import os

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")

import tempfile
import unittest
from dataclasses import replace
from pathlib import Path

from PyQt6.QtGui import QColor, QImage

from rendering import (
    RenderConfig,
    _document,
    ensure_application,
    load_preset,
    render_image,
    resource_path,
    save_image,
    save_preset,
)


class RenderingTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = ensure_application(headless=True)

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.directory = Path(self.temp.name).resolve()
        background = QImage(240, 180, QImage.Format.Format_ARGB32)
        background.fill(QColor("white"))
        self.background = self.directory / "paper.png"
        self.assertTrue(background.save(str(self.background)))
        self.config = RenderConfig(
            text="你好 <b> & 文字\n第二行\n第三行",
            bg_path=str(self.background),
            x=10,
            y=10,
            rect_width=200,
            rect_height=35,
        )

    def test_literal_text_clip_and_original_canvas(self):
        document = _document(self.config)
        self.assertEqual(document.toPlainText(), self.config.text)
        result = render_image(self.config)
        self.assertEqual((result.width(), result.height()), (240, 180))
        self.assertTrue(
            any(
                result.pixelColor(x, y) != QColor("white")
                for y in range(10, 45)
                for x in range(10, 210)
            )
        )
        self.assertTrue(
            all(
                result.pixelColor(x, y) == QColor("white")
                for y in range(45, 180)
                for x in range(240)
            )
        )
        self.assertTrue(
            all(
                result.pixelColor(x, y) == QColor("white")
                for y in range(180)
                for x in range(210, 240)
            )
        )
        self.assertEqual(
            render_image(replace(self.config, text="")).pixelColor(20, 20), QColor("white")
        )

    def test_preset_roundtrip_and_relative_resources(self):
        path = self.directory / "preset.json"
        save_preset(self.config, path)
        loaded = load_preset(path)
        self.assertEqual(loaded, self.config)
        self.assertIn('"bg_path": "paper.png"', path.read_text())
        path.write_text('{"font_size": "large"}')
        with self.assertRaises(ValueError):
            load_preset(path)

    def test_invalid_inputs_fail_cleanly(self):
        for updates in (
            {"font_path": str(self.directory / "missing.ttf")},
            {"bg_path": str(self.directory / "missing.png")},
            {"font_size": 0},
            {"rect_height": -1},
            {"font_color": "invalid color"},
            {"line_spacing": float("nan")},
        ):
            with self.subTest(updates=updates), self.assertRaises(ValueError):
                render_image(replace(self.config, **updates))
        bad_font = self.directory / "broken.ttf"
        bad_font.write_text("not a font")
        with self.assertRaises(ValueError):
            render_image(replace(self.config, font_path=str(bad_font)))

    def test_atomic_export_and_explicit_overwrite(self):
        image = render_image(self.config)
        path = self.directory / "result.png"
        self.assertEqual(save_image(image, path), path)
        self.assertEqual(QImage(str(path)).size(), image.size())
        old_data = path.read_bytes()
        with self.assertRaises(FileExistsError):
            save_image(image, path)
        self.assertEqual(old_data, path.read_bytes())
        save_image(image, path, overwrite=True)
        with self.assertRaises(ValueError):
            save_image(image, self.directory / "result.txt")
        nested = self.directory / "nested" / "result.png"
        save_image(image, nested)
        self.assertTrue(nested.is_file())
        with self.assertRaises(OSError):
            save_image(image, path / "result.png")

    def test_bundled_assets_do_not_depend_on_working_directory(self):
        previous = Path.cwd()
        try:
            os.chdir(self.directory)
            result = render_image(RenderConfig(text="跨目录启动"))
            self.assertEqual(result.size(), QImage(str(resource_path("bgs/letter.png"))).size())
        finally:
            os.chdir(previous)


if __name__ == "__main__":
    unittest.main()
