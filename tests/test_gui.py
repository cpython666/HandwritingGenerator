import os

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

import json
import subprocess
import sys
import tempfile
import unittest
from dataclasses import replace
from pathlib import Path
from unittest.mock import patch

from PyQt6.QtCore import QPoint, QPointF, QRect, QRectF, Qt
from PyQt6.QtGui import QColor, QImage, QWheelEvent
from PyQt6.QtWidgets import QApplication, QGraphicsTextItem, QMessageBox

from main import HandFontWindow
from rendering import ROOT, RenderConfig, load_preset, render_image, save_preset


class GUITests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = QApplication.instance() or QApplication([])
        if not isinstance(cls.app, QApplication):
            raise RuntimeError("GUI tests need QApplication; run them before QGuiApplication tests")

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.directory = Path(self.temp.name).resolve()
        paper = QImage(360, 240, QImage.Format.Format_ARGB32)
        paper.fill(QColor("white"))
        background = self.directory / "paper.png"
        self.assertTrue(paper.save(str(background)))
        self.config = RenderConfig(
            text="你好 <世界> & 朋友\n第二行",
            bg_path=str(background),
            x=10,
            y=10,
            rect_width=320,
            rect_height=200,
        )
        self.window = HandFontWindow()
        self.addCleanup(self.window.close)
        self.addCleanup(self.window.render_timer.stop)
        self.window.use_config(self.config)

    def assert_image_equal(self, actual, expected):
        self.assertEqual(actual.size(), expected.size())
        actual = actual.convertToFormat(QImage.Format.Format_ARGB32)
        expected = expected.convertToFormat(QImage.Format.Format_ARGB32)
        self.assertEqual(
            actual.constBits().asstring(actual.sizeInBytes()),
            expected.constBits().asstring(expected.sizeInBytes()),
        )

    def test_export_uses_page_size_excludes_measurements_and_overwrites(self):
        self.assertEqual(self.window.view.sceneRect(), QRectF(0, 0, 360, 240))
        marker = self.window.view.scene().addRect(QRectF(-50, -50, 500, 400))
        label = QGraphicsTextItem("测量辅助线不应导出", marker)
        label.setDefaultTextColor(QColor("red"))
        output = self.directory / "result.png"
        output.write_bytes(b"previous output; save dialog has confirmed replacement")
        with patch("main.QFileDialog.getSaveFileName", return_value=(str(output), "PNG (*.png)")):
            self.window.export_image()
        exported = QImage(str(output))
        self.assertEqual((exported.width(), exported.height()), (360, 240))
        self.assert_image_equal(exported, render_image(self.config))
        self.assert_image_equal(self.window.view.page.pixmap().toImage(), exported)
        self.assertIn(marker, self.window.view.scene().items())

    def test_export_flushes_pending_text_change(self):
        old_image = self.window.image.copy()
        new_text = "刚刚输入的新内容\n应立即出现在导出图片"
        self.window.text_edit.setPlainText(new_text)
        self.assertTrue(self.window.render_timer.isActive())
        self.assert_image_equal(self.window.image, old_image)
        output = self.directory / "latest.png"
        with patch("main.QFileDialog.getSaveFileName", return_value=(str(output), "PNG (*.png)")):
            self.window.export_image()
        self.assertFalse(self.window.render_timer.isActive())
        self.assertEqual(self.window.config.text, new_text)
        self.assert_image_equal(
            QImage(str(output)), render_image(replace(self.config, text=new_text))
        )

    def test_added_extension_requires_confirmation_before_overwrite(self):
        selected = self.directory / "existing"
        output = selected.with_suffix(".png")
        original = b"keep this existing file until replacement is confirmed"
        output.write_bytes(original)
        with patch("main.QFileDialog.getSaveFileName", return_value=(str(selected), "PNG (*.png)")):
            with patch(
                "main.QMessageBox.question", return_value=QMessageBox.StandardButton.No
            ) as question:
                self.window.export_image()
            question.assert_called_once()
            self.assertEqual(question.call_args.args[-1], QMessageBox.StandardButton.No)
            self.assertEqual(output.read_bytes(), original)
            self.assertFalse(selected.exists())
            with patch(
                "main.QMessageBox.question", return_value=QMessageBox.StandardButton.Yes
            ) as question:
                self.window.export_image()
            question.assert_called_once()
        self.assert_image_equal(QImage(str(output)), render_image(self.config))

    def test_invalid_font_keeps_last_preview_and_reports_failure(self):
        last_image = self.window.image.copy()
        invalid = replace(self.config, font_path=str(self.directory / "missing.ttf"))
        with patch("main.QMessageBox.warning") as warning:
            self.window.use_config(invalid)
        warning.assert_called_once()
        self.assertEqual(self.window.current_config(), self.config)
        self.assert_image_equal(self.window.image, last_image)
        self.window.apply_config(invalid)
        self.assertFalse(self.window.update_preview())
        self.assertIn("预览失败", self.window.statusBar().currentMessage())
        self.assert_image_equal(self.window.image, last_image)

    def test_preset_parameters_survive_gui_roundtrip(self):
        for spacing, line_height in ((-1, 0.1), (2, 5), (0.12345, 1.23456)):
            with self.subTest(spacing=spacing, line_height=line_height):
                config = replace(
                    self.config,
                    font_spacing=spacing,
                    line_spacing=line_height,
                    font_size=31,
                    font_weight=735,
                    font_color="#234567",
                    x=18,
                    y=23,
                    rect_width=297,
                    rect_height=159,
                )
                source = self.directory / "source.json"
                destination = self.directory / "saved.json"
                save_preset(config, source)
                with patch(
                    "main.QFileDialog.getOpenFileName", return_value=(str(source), "JSON (*.json)")
                ):
                    self.window.open_preset()
                self.assertEqual(self.window.current_config(), config)
                self.assertEqual(self.window.text_edit.toPlainText(), config.text)
                self.assertEqual(self.window.color_button.text(), config.font_color)
                with patch(
                    "main.QFileDialog.getSaveFileName",
                    return_value=(str(destination), "JSON (*.json)"),
                ):
                    self.window.write_preset()
                self.assertEqual(load_preset(destination), config)

    def test_tabs_keep_export_accessible_in_small_windows(self):
        self.window.show()
        for width, height in ((900, 620), (840, 560)):
            self.window.resize(width, height)
            for index, control in enumerate(
                (self.window.text_edit, self.window.controls["rect_width"], self.window.paper_combo)
            ):
                with self.subTest(size=(width, height), tab=index):
                    self.window.tabs.setCurrentIndex(index)
                    self.app.processEvents()
                    self.assertEqual((self.window.width(), self.window.height()), (width, height))
                    self.assertTrue(control.isVisible())
                    button = self.window.export_button
                    self.assertFalse(button.visibleRegion().isEmpty())
                    self.assertTrue(
                        self.window.rect().contains(
                            QRect(button.mapTo(self.window, QPoint()), button.size())
                        )
                    )
                    self.assertEqual(self.window.current_config(), self.config)
                    self.assertGreater(self.window.view.viewport().width(), 200)

    def test_boundary_overlay_does_not_resize_or_export_with_page(self):
        self.window.use_config(replace(self.config, rect_width=600, rect_height=400))
        self.assertEqual(self.window.current_config().rect_width, 600)
        self.assertEqual(self.window.current_config().rect_height, 400)
        self.window.controls["x"].setValue(400)
        self.assertEqual(self.window.current_config().x, 400)
        self.window.controls["x"].setValue(self.config.x)
        self.window.update_preview()
        expected = self.window.image.copy()
        self.window.boundary_button.setChecked(True)
        self.assertTrue(self.window.view.region.isVisible())
        self.assertGreater(self.window.view.region.rect().right(), expected.width())
        self.assertEqual(self.window.view.sceneRect(), QRectF(expected.rect()))
        output = self.directory / "with-boundary.png"
        with patch("main.QFileDialog.getSaveFileName", return_value=(str(output), "PNG (*.png)")):
            self.window.export_image()
        self.assert_image_equal(QImage(str(output)), expected)
        self.window.boundary_button.setChecked(False)
        self.assertFalse(self.window.view.region.isVisible())

    def test_resize_fits_page_until_user_selects_zoom(self):
        self.window.resize(1200, 800)
        self.window.show()
        self.app.processEvents()
        view = self.window.view
        view.fit_page()
        large_zoom = view.transform().m11()
        self.window.resize(840, 560)
        self.app.processEvents()
        self.assertTrue(view.auto_fit)
        self.assertLess(view.transform().m11(), large_zoom)
        page = view.mapFromScene(view.page.boundingRect()).boundingRect()
        self.assertLessEqual(page.width(), view.viewport().width() + 2)
        self.assertLessEqual(page.height(), view.viewport().height() + 2)
        view.set_zoom(2)
        self.window.resize(1200, 800)
        self.app.processEvents()
        self.assertFalse(view.auto_fit)
        self.assertAlmostEqual(view.transform().m11(), 2)

    def test_wheel_zoom_handles_touchpad_and_only_control_modifier(self):
        self.window.show()
        self.app.processEvents()
        view = self.window.view

        def scroll(modifier, pixels=0, angle=0):
            point = view.viewport().rect().center()
            event = QWheelEvent(
                QPointF(point),
                QPointF(view.viewport().mapToGlobal(point)),
                QPoint(0, pixels),
                QPoint(0, angle),
                Qt.MouseButton.NoButton,
                modifier,
                Qt.ScrollPhase.ScrollUpdate,
                False,
            )
            self.app.sendEvent(view.viewport(), event)

        view.set_zoom(1)
        for modifier in (Qt.KeyboardModifier.NoModifier, Qt.KeyboardModifier.MetaModifier):
            scroll(modifier, pixels=120, angle=120)
            self.assertAlmostEqual(view.transform().m11(), 1)
        scroll(Qt.KeyboardModifier.ControlModifier, pixels=12)
        small_step = view.transform().m11()
        self.assertGreater(small_step, 1)
        self.assertLess(small_step, 1.1)
        for _ in range(9):
            scroll(Qt.KeyboardModifier.ControlModifier, pixels=12)
        accumulated = view.transform().m11()
        view.set_zoom(1)
        scroll(Qt.KeyboardModifier.ControlModifier, pixels=120)
        self.assertAlmostEqual(view.transform().m11(), accumulated)
        view.set_zoom(1)
        scroll(Qt.KeyboardModifier.ControlModifier, angle=120)
        self.assertGreater(view.transform().m11(), small_step)
        last_zoom = view.transform().m11()
        scroll(Qt.KeyboardModifier.ControlModifier)
        self.assertEqual(view.transform().m11(), last_zoom)

    def test_high_dpi_preserves_output_pixels_and_export_access(self):
        script = """
import hashlib
import json
from PyQt6.QtCore import QPoint, QRect
from PyQt6.QtWidgets import QApplication
from main import HandFontWindow

app = QApplication([])
window = HandFontWindow()
window.resize(900, 620)
window.show()
visible = []
for index in range(window.tabs.count()):
    window.tabs.setCurrentIndex(index)
    app.processEvents()
    button = window.export_button
    visible.append(not button.visibleRegion().isEmpty() and window.rect().contains(
        QRect(button.mapTo(window, QPoint()), button.size())))
image = window.image
print(json.dumps({
    'size': [image.width(), image.height()],
    'sha256': hashlib.sha256(image.constBits().asstring(image.sizeInBytes())).hexdigest(),
    'export_visible': visible,
    'window_size': [window.width(), window.height()],
    'device_pixel_ratio': window.devicePixelRatioF(),
}))
window.close()
"""
        baseline = None
        for scale in ("1", "1.5", "2"):
            with self.subTest(scale=scale):
                result = subprocess.run(
                    [sys.executable, "-c", script],
                    cwd=ROOT,
                    env={**os.environ, "QT_QPA_PLATFORM": "offscreen", "QT_SCALE_FACTOR": scale},
                    capture_output=True,
                    text=True,
                    timeout=15,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                data = json.loads(result.stdout)
                self.assertEqual(data["window_size"], [900, 620])
                self.assertEqual(data["export_visible"], [True, True, True])
                self.assertAlmostEqual(data["device_pixel_ratio"], float(scale))
                rendered = data["size"], data["sha256"]
                if baseline is None:
                    baseline = rendered
                self.assertEqual(rendered, baseline)


if __name__ == "__main__":
    unittest.main()
