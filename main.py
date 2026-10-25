"""Desktop editor; rendering and file formats are shared with CLI and MCP."""

import sys
from dataclasses import replace
from pathlib import Path

from PyQt6.QtCore import QRectF, QSize, Qt, QTimer, pyqtSignal
from PyQt6.QtGui import QColor, QIcon, QImage, QKeySequence, QPainter, QPen, QPixmap, QShortcut
from PyQt6.QtWidgets import (
    QApplication,
    QColorDialog,
    QComboBox,
    QDialog,
    QDoubleSpinBox,
    QFileDialog,
    QFormLayout,
    QFrame,
    QGraphicsDropShadowEffect,
    QGraphicsRectItem,
    QGraphicsScene,
    QGraphicsTextItem,
    QGraphicsView,
    QHBoxLayout,
    QLabel,
    QMainWindow,
    QMessageBox,
    QPushButton,
    QScrollArea,
    QSlider,
    QSpinBox,
    QSplitter,
    QTabWidget,
    QTextEdit,
    QVBoxLayout,
    QWidget,
)

from rendering import (
    MAX_DIMENSION,
    MAX_PIXELS,
    RenderConfig,
    list_assets,
    load_preset,
    render_image,
    resource_path,
    save_image,
    save_preset,
)
from ui_style import apply_theme

IMAGE_FILTER = "图片 (*.png *.jpg *.jpeg *.webp *.bmp)"
SAVE_FILTER = "PNG (*.png);;JPEG (*.jpg);;WebP (*.webp)"
SAMPLE_TEXT = "把一颗星，藏进字里行间。\n\n愿每一句未曾说出口的话，\n都能在纸上，找到温柔的回响。"


def choose_save_path(parent, title, suggested, filters):
    path, selected = QFileDialog.getSaveFileName(parent, title, suggested, filters)
    if path and not Path(path).suffix:
        suffix = {"JPEG (*.jpg)": ".jpg", "WebP (*.webp)": ".webp", "JSON (*.json)": ".json"}
        path += suffix.get(selected, ".png")
        # The native dialog only confirmed the name before we added its extension.
        if (
            Path(path).exists()
            and QMessageBox.question(
                parent,
                "覆盖文件",
                f"{path} 已存在，是否覆盖？",
                QMessageBox.StandardButton.Yes | QMessageBox.StandardButton.No,
                QMessageBox.StandardButton.No,
            )
            != QMessageBox.StandardButton.Yes
        ):
            return ""
    return path


class ImageCreator(QDialog):
    created = pyqtSignal(str)

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setWindowTitle("创建纯色背景")
        self.color = QColor("white")
        layout = QFormLayout(self)
        self.width_input = QSpinBox()
        self.height_input = QSpinBox()
        for control, value in ((self.width_input, 724), (self.height_input, 1024)):
            control.setRange(1, MAX_DIMENSION)
            control.setValue(value)
            control.setSuffix(" px")
        self.alpha = QSpinBox()
        self.alpha.setRange(0, 255)
        self.alpha.setValue(255)
        self.color_button = QPushButton("选择颜色：#ffffff")
        self.color_button.clicked.connect(self.choose_color)
        layout.addRow("宽度", self.width_input)
        layout.addRow("高度", self.height_input)
        layout.addRow("不透明度", self.alpha)
        layout.addRow(self.color_button)
        create = QPushButton("保存并使用")
        create.clicked.connect(self.create_image)
        layout.addRow(create)

    def choose_color(self):
        color = QColorDialog.getColor(self.color, self)
        if color.isValid():
            self.color = color
            self.color_button.setText(f"选择颜色：{color.name()}")

    def create_image(self):
        width, height = self.width_input.value(), self.height_input.value()
        if width * height > MAX_PIXELS:
            QMessageBox.warning(self, "图片过大", f"像素总数不能超过 {MAX_PIXELS:,}。")
            return
        path = choose_save_path(self, "保存背景", "background.png", "PNG (*.png)")
        if not path:
            return
        self.color.setAlpha(self.alpha.value())
        image = QImage(width, height, QImage.Format.Format_ARGB32_Premultiplied)
        image.fill(self.color)
        try:
            saved = save_image(image, path, overwrite=True)
        except (OSError, ValueError) as exc:
            QMessageBox.warning(self, "保存失败", str(exc))
            return
        self.created.emit(str(saved))
        self.accept()


class GraphicsView(QGraphicsView):
    zoom_changed = pyqtSignal(float)

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setScene(QGraphicsScene(self))
        self.page = self.scene().addPixmap(QPixmap())
        shadow = QGraphicsDropShadowEffect(self)
        shadow.setBlurRadius(22)
        shadow.setOffset(0, 3)
        shadow.setColor(QColor(46, 35, 68, 35))
        self.page.setGraphicsEffect(shadow)
        self.setBackgroundBrush(QColor("#eeedf3"))
        self.setFrameShape(QFrame.Shape.NoFrame)
        self.setRenderHints(
            QPainter.RenderHint.Antialiasing | QPainter.RenderHint.SmoothPixmapTransform
        )
        self.setDragMode(QGraphicsView.DragMode.ScrollHandDrag)
        self.setTransformationAnchor(QGraphicsView.ViewportAnchor.AnchorUnderMouse)
        self.origin = None
        self.drawing_rect = None
        self.auto_fit = True
        self.region = self.scene().addRect(QRectF())
        pen = QPen(QColor("#9679d0"), 1, Qt.PenStyle.DashLine)
        pen.setCosmetic(True)
        self.region.setPen(pen)
        self.region.setZValue(2)
        self.region.setAcceptedMouseButtons(Qt.MouseButton.NoButton)
        self.region.hide()

    def set_image(self, image):
        self.page.setPixmap(QPixmap.fromImage(image))
        self.setSceneRect(QRectF(image.rect()))

    def fit_page(self):
        if not self.page.pixmap().isNull():
            self.auto_fit = True
            self.fitInView(self.page, Qt.AspectRatioMode.KeepAspectRatio)
            self.scale(0.94, 0.94)
            self.zoom_changed.emit(self.transform().m11())

    def set_zoom(self, value):
        self.auto_fit = False
        value = max(0.05, min(value, 16))
        self.scale(value / self.transform().m11(), value / self.transform().m11())
        self.zoom_changed.emit(self.transform().m11())

    def resizeEvent(self, event):
        super().resizeEvent(event)
        if self.auto_fit:
            self.fit_page()

    def wheelEvent(self, event):
        if event.modifiers() & Qt.KeyboardModifier.ControlModifier:
            delta = event.pixelDelta().y() or event.angleDelta().y()
            if delta:
                self.set_zoom(self.transform().m11() * 1.2 ** (max(-240, min(delta, 240)) / 120))
            event.accept()
        else:
            super().wheelEvent(event)

    def mousePressEvent(self, event):
        if event.button() == Qt.MouseButton.RightButton:
            self.origin = self.mapToScene(event.pos())
            self.drawing_rect = self.scene().addRect(QRectF(self.origin, self.origin))
            self.drawing_rect.setZValue(1)
            event.accept()
        else:
            super().mousePressEvent(event)

    def mouseMoveEvent(self, event):
        if self.origin is not None and self.drawing_rect is not None:
            self.drawing_rect.setRect(
                QRectF(self.origin, self.mapToScene(event.pos())).normalized()
            )
        else:
            super().mouseMoveEvent(event)

    def mouseReleaseEvent(self, event):
        if event.button() == Qt.MouseButton.RightButton and self.drawing_rect is not None:
            rect = QRectF(self.origin, self.mapToScene(event.pos())).normalized()
            self.drawing_rect.setRect(rect)
            label = QGraphicsTextItem(
                f"({rect.x():.0f}, {rect.y():.0f}) · {rect.width():.0f} × {rect.height():.0f}",
                self.drawing_rect,
            )
            label.setDefaultTextColor(QColor("#c0392b"))
            label.setPos(rect.topLeft())
            self.origin = self.drawing_rect = None
            event.accept()
        else:
            super().mouseReleaseEvent(event)

    def mouseDoubleClickEvent(self, event):
        item = self.itemAt(event.pos())
        if isinstance(item, QGraphicsTextItem):
            item = item.parentItem()
        if isinstance(item, QGraphicsRectItem) and item is not self.region:
            self.scene().removeItem(item)
            if item is self.drawing_rect:
                self.origin = self.drawing_rect = None
        else:
            super().mouseDoubleClickEvent(event)


class DecimalSpinBox(QDoubleSpinBox):
    def textFromValue(self, value):
        text = super().textFromValue(value)
        separator = self.locale().decimalPoint()
        return (
            text.rstrip(self.locale().zeroDigit()).rstrip(separator) if separator in text else text
        )


class HandFontWindow(QMainWindow):
    def __init__(self):
        super().__init__()
        apply_theme(QApplication.instance())
        self.setWindowTitle("星笺 · 手写图片生成器")
        self.setWindowIcon(QIcon(str(resource_path("logo.png"))))
        self.setMinimumSize(840, 560)
        screen = self.screen().availableGeometry()
        self.resize(min(1280, screen.width() - 48), min(880, screen.height() - 80))
        self.config = RenderConfig(text=SAMPLE_TEXT)
        self._loading = True
        self.image = None
        self.render_timer = QTimer(self)
        self.render_timer.setSingleShot(True)
        self.render_timer.setInterval(120)
        self.render_timer.timeout.connect(self.update_preview)
        self.view = GraphicsView(self)
        self.controls = {}
        self.sliders = {}
        self.assets = list_assets()

        body = QWidget()
        body.setObjectName("workspace")
        root = QVBoxLayout(body)
        root.setContentsMargins(0, 0, 0, 0)
        root.setSpacing(0)
        header = QFrame()
        header.setObjectName("header")
        bar = QHBoxLayout(header)
        bar.setContentsMargins(24, 16, 24, 16)
        mark = QLabel("星")
        mark.setAlignment(Qt.AlignmentFlag.AlignCenter)
        mark.setFixedSize(42, 42)
        mark.setStyleSheet(
            "background: #6550bd; color: white; border-radius: 12px; font-size: 22px;"
        )
        bar.addWidget(mark)
        brand = QVBoxLayout()
        brand.setSpacing(0)
        brand.addWidget(self.label("星笺", "brand"))
        brand.addWidget(self.label("让文字，带一点星光。", "eyebrow"))
        bar.addLayout(brand)
        bar.addStretch()
        bar.addWidget(self.button("关于星笺", self.show_about))
        self.open_button = self.button("打开预设", self.open_preset)
        self.save_button = self.button("保存预设", self.write_preset)
        self.export_button = self.button("导出图片", self.export_image)
        self.export_button.setObjectName("primary")
        for button in (self.open_button, self.save_button, self.export_button):
            bar.addWidget(button)
        root.addWidget(header)

        self.splitter = QSplitter(Qt.Orientation.Horizontal)
        self.splitter.setChildrenCollapsible(False)
        self.splitter.setHandleWidth(14)
        inspector = QFrame()
        inspector.setObjectName("inspector")
        inspector.setMinimumWidth(320)
        inspector.setMaximumWidth(440)
        inspector_layout = QVBoxLayout(inspector)
        inspector_layout.setContentsMargins(0, 0, 0, 0)
        self.tabs = QTabWidget()
        self.tabs.setDocumentMode(True)
        inspector_layout.addWidget(self.tabs)
        self.splitter.addWidget(inspector)

        content = self.tab_page("内容")
        introduction = self.label("把想说的话，写成一页有温度的纸笺。", "hint")
        introduction.setWordWrap(True)
        content.addWidget(introduction)
        heading = QHBoxLayout()
        heading.addWidget(self.label("写下你的文字", "sectionTitle"))
        heading.addStretch()
        self.character_count = self.label("0 字", "count")
        heading.addWidget(self.character_count)
        content.addLayout(heading)
        self.text_edit = QTextEdit()
        self.text_edit.setAcceptRichText(False)
        self.text_edit.setPlaceholderText("写一封信，留一段诗，或记下今晚的星光…")
        self.text_edit.setMinimumHeight(180)
        self.text_edit.setMaximumHeight(280)
        self.text_edit.setAccessibleName("要生成的文字")
        self.text_edit.textChanged.connect(self.schedule_preview)
        self.text_edit.textChanged.connect(self.update_character_count)
        content.addWidget(self.text_edit)
        content.addSpacing(12)
        content.addWidget(self.label("笔迹与墨色", "sectionTitle"))
        content.addWidget(self.label("选择一种喜欢的手写风格", "hint"))
        font_row = QHBoxLayout()
        self.font_combo = QComboBox()
        self.font_combo.setSizeAdjustPolicy(
            QComboBox.SizeAdjustPolicy.AdjustToMinimumContentsLengthWithIcon
        )
        self.font_combo.setMinimumContentsLength(10)
        self.font_combo.setAccessibleName("手写字体")
        for path in self.assets["fonts"]:
            self.font_combo.addItem(Path(path).stem, path)
        self.font_combo.currentIndexChanged.connect(self.schedule_preview)
        font_row.addWidget(self.font_combo, 1)
        font_row.addWidget(self.button("导入…", self.choose_font))
        content.addLayout(font_row)
        type_form = QFormLayout()
        type_form.setSpacing(12)
        self.add_control(type_form, "font_size", "字号 · pt", 1, 200, 1)
        self.add_control(type_form, "font_weight", "字重", 100, 900, 100)
        content.addLayout(type_form)
        ink = QHBoxLayout()
        self.color_swatch = QLabel()
        self.color_swatch.setFixedSize(24, 24)
        ink.addWidget(self.color_swatch)
        self.color_button = self.button("", self.choose_color)
        self.color_button.setAccessibleName("选择文字颜色")
        ink.addWidget(self.color_button, 1)
        content.addLayout(ink)
        content.addStretch()
        content.addWidget(self.label("一页纸笺，收藏日常的微光。", "hint"))

        layout_page = self.tab_page("排版")
        layout_page.addWidget(self.label("文字区域", "sectionTitle"))
        layout_page.addWidget(self.label("以纸张左上角为起点，单位为像素", "hint"))
        region_form = QFormLayout()
        region_form.setSpacing(14)
        for key, label, minimum in (
            ("x", "左边距", 0),
            ("y", "上边距", 0),
            ("rect_width", "区域宽度", 1),
            ("rect_height", "区域高度", 1),
        ):
            self.add_control(region_form, key, label, minimum, MAX_DIMENSION, 1)
        layout_page.addLayout(region_form)
        layout_page.addSpacing(20)
        layout_page.addWidget(self.label("书写节奏", "sectionTitle"))
        spacing_form = QFormLayout()
        spacing_form.setSpacing(14)
        self.add_control(spacing_form, "font_spacing", "附加字距 · em", -1.0, 2.0, 0.05)
        self.add_control(spacing_form, "line_spacing", "行距 · 倍数", 0.1, 5.0, 0.05)
        layout_page.addLayout(spacing_form)
        layout_page.addSpacing(12)
        note = self.label(
            "打开画布上的「文字边界」，可查看排版范围。超出区域的文字会裁切。", "hint"
        )
        note.setWordWrap(True)
        layout_page.addWidget(note)
        layout_page.addStretch()

        paper_page = self.tab_page("纸张")
        paper_page.addWidget(self.label("为文字选一张纸", "sectionTitle"))
        paper_page.addWidget(self.label("内置信纸，也可以使用自己的图片", "hint"))
        self.paper_combo = QComboBox()
        self.paper_combo.setAccessibleName("背景纸张")
        self.paper_combo.setSizeAdjustPolicy(
            QComboBox.SizeAdjustPolicy.AdjustToMinimumContentsLengthWithIcon
        )
        self.paper_combo.setMinimumContentsLength(10)
        for path in self.assets["backgrounds"]:
            self.paper_combo.addItem(Path(path).stem, path)
        self.paper_combo.currentIndexChanged.connect(self.choose_builtin_background)
        paper_page.addWidget(self.paper_combo)
        self.paper_thumbnail = self.label("", "paperThumbnail")
        self.paper_thumbnail.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.paper_thumbnail.setMinimumHeight(200)
        paper_page.addWidget(self.paper_thumbnail)
        self.background_label = self.label("", "paperName")
        self.background_label.setWordWrap(True)
        paper_page.addWidget(self.background_label)
        paper_page.addWidget(self.button("从文件选择背景…", self.choose_background))
        paper_page.addWidget(self.button("创建纯色纸张", self.create_background))
        paper_page.addStretch()
        paper_page.addWidget(self.label("导出尺寸与背景原图保持一致。", "hint"))

        preview = QFrame()
        preview.setObjectName("preview")
        preview.setMinimumWidth(420)
        canvas = QVBoxLayout(preview)
        canvas.setContentsMargins(14, 14, 14, 12)
        canvas.setSpacing(12)
        preview_title = QHBoxLayout()
        preview_title.addWidget(self.label("画布预览", "sectionTitle"))
        preview_title.addStretch()
        self.canvas_size = self.label("", "badge")
        preview_title.addWidget(self.canvas_size)
        canvas.addLayout(preview_title)
        toolbar = QHBoxLayout()
        toolbar.setSpacing(6)
        self.boundary_button = self.button("文字边界", None)
        self.boundary_button.setCheckable(True)
        self.boundary_button.toggled.connect(self.view.region.setVisible)
        toolbar.addWidget(self.boundary_button)
        toolbar.addStretch()
        toolbar.addWidget(
            self.button(
                "−", lambda: self.view.set_zoom(self.view.transform().m11() / 1.2), "缩小画布"
            )
        )
        self.zoom_label = self.label("100%")
        self.zoom_label.setMinimumWidth(44)
        self.zoom_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        self.view.zoom_changed.connect(lambda zoom: self.zoom_label.setText(f"{zoom:.0%}"))
        toolbar.addWidget(self.zoom_label)
        toolbar.addWidget(
            self.button(
                "+", lambda: self.view.set_zoom(self.view.transform().m11() * 1.2), "放大画布"
            )
        )
        toolbar.addWidget(self.button("1:1", lambda: self.view.set_zoom(1), "100% 画布缩放"))
        toolbar.addWidget(self.button("适应", self.view.fit_page, "让整张纸适应窗口"))
        canvas.addLayout(toolbar)
        canvas.addWidget(self.view, 1)
        modifier = "⌘" if sys.platform == "darwin" else "Ctrl"
        guide = self.label(f"拖动画布平移  ·  {modifier} + 滚轮缩放  ·  右键拖动测量", "hint")
        guide.setWordWrap(True)
        canvas.addWidget(guide)
        self.splitter.addWidget(preview)
        self.splitter.setStretchFactor(1, 1)
        self.splitter.setSizes([350, 830])
        workspace = QVBoxLayout()
        workspace.setContentsMargins(20, 18, 20, 8)
        workspace.addWidget(self.splitter)
        root.addLayout(workspace, 1)
        self.setCentralWidget(body)
        self.shortcuts = []
        for key, callback, button in (
            (QKeySequence.StandardKey.Open, self.open_preset, self.open_button),
            (QKeySequence.StandardKey.SaveAs, self.write_preset, self.save_button),
            (QKeySequence.StandardKey.Save, self.export_image, self.export_button),
            (QKeySequence.StandardKey.Close, self.close, None),
        ):
            shortcut = QShortcut(QKeySequence(key), self)
            shortcut.activated.connect(callback)
            self.shortcuts.append(shortcut)
            if button is not None:
                button.setToolTip(
                    f"{button.text()} · {shortcut.key().toString(QKeySequence.SequenceFormat.NativeText)}"
                )
        self.apply_config(self.config)
        self.update_preview()
        QTimer.singleShot(0, self.view.fit_page)

    def show_about(self):
        QMessageBox.about(
            self,
            "关于星笺",
            "<h2>星笺</h2>"
            "<p>手写图片生成器 · 让文字，带一点星光。</p>"
            "<p>把想说的话，写成一页有温度的纸笺。<br>"
            "星笺将文字化作手写模样，陪你写一封信、留一段诗，"
            "也收藏日常里微小而明亮的心事。</p>"
            "<p>挑选笔迹与纸张，调整墨色与留白，实时预览并导出图片；"
            "把喜欢的排版存为预设，留给下一次书写。</p>"
            "<p>文字与图片均在本地处理，离线也能安心书写。</p>",
        )

    @staticmethod
    def label(text, name=""):
        label = QLabel(text)
        label.setObjectName(name)
        return label

    @staticmethod
    def button(text, callback, description=""):
        button = QPushButton(text)
        button.setCursor(Qt.CursorShape.PointingHandCursor)
        if callback is not None:
            button.clicked.connect(callback)
        button.setAccessibleName(description or text)
        if description:
            button.setToolTip(description)
        return button

    def tab_page(self, title):
        page = QWidget()
        page.setObjectName("tabPage")
        layout = QVBoxLayout(page)
        layout.setContentsMargins(18, 20, 18, 20)
        layout.setSpacing(10)
        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarPolicy.ScrollBarAlwaysOff)
        scroll.setWidget(page)
        self.tabs.addTab(scroll, title)
        return layout

    def add_control(self, form, key, label, low, high, step):
        decimal = isinstance(step, float)
        spin = DecimalSpinBox() if decimal else QSpinBox()
        if decimal:
            spin.setDecimals(6)
        spin.setRange(low, high)
        spin.setSingleStep(step)
        spin.setAccessibleName(label)
        spin.setFocusPolicy(Qt.FocusPolicy.StrongFocus)
        spin.setMinimumWidth(88)
        spin.valueChanged.connect(self.schedule_preview)
        self.controls[key] = spin
        if decimal:
            form.addRow(label, spin)
        else:
            row = QHBoxLayout()
            slider = QSlider(Qt.Orientation.Horizontal)
            slider.setRange(low, high)
            slider.setSingleStep(step)
            slider.setAccessibleName(label)
            self.sliders[key] = slider
            slider.valueChanged.connect(spin.setValue)

            def sync_slider(value):
                slider.setMaximum(max(slider.maximum(), value))
                slider.setValue(value)

            spin.valueChanged.connect(sync_slider)
            row.addWidget(slider, 1)
            row.addWidget(spin)
            form.addRow(label, row)

    def update_character_count(self):
        self.character_count.setText(f"{len(self.text_edit.toPlainText()):,} 字")

    def choose_builtin_background(self):
        if not self._loading and self.paper_combo.currentData():
            self.use_config(replace(self.current_config(), bg_path=self.paper_combo.currentData()))

    def update_paper(self):
        index = self.paper_combo.findData(self.config.bg_path)
        if index < 0:
            self.paper_combo.addItem(Path(self.config.bg_path).stem, self.config.bg_path)
            index = self.paper_combo.count() - 1
        self.paper_combo.setCurrentIndex(index)
        thumbnail = QPixmap(self.config.bg_path)
        if not thumbnail.isNull():
            ratio = self.devicePixelRatioF()
            thumbnail = thumbnail.scaled(
                QSize(round(220 * ratio), round(200 * ratio)),
                Qt.AspectRatioMode.KeepAspectRatio,
                Qt.TransformationMode.SmoothTransformation,
            )
            thumbnail.setDevicePixelRatio(ratio)
            self.paper_thumbnail.setPixmap(thumbnail)

    def update_ink(self):
        self.color_button.setText(self.config.font_color)
        self.color_swatch.setStyleSheet(
            f"background: {QColor(self.config.font_color).name()}; border: 1px solid #d0cbd8; border-radius: 12px;"
        )

    def apply_config(self, config):
        self._loading = True
        self.config = config
        self.background_label.setText(f"背景：{Path(config.bg_path).name}")
        self.background_label.setToolTip(config.bg_path)
        index = self.font_combo.findData(config.font_path)
        if index < 0:
            self.font_combo.addItem(Path(config.font_path).name, config.font_path)
            index = self.font_combo.count() - 1
        self.font_combo.setCurrentIndex(index)
        self.update_ink()
        self.update_paper()
        self.text_edit.setPlainText(config.text)
        for key, control in self.controls.items():
            control.setValue(getattr(config, key))
        self._loading = False
        self.schedule_preview()

    def current_config(self):
        return replace(
            self.config,
            text=self.text_edit.toPlainText(),
            font_path=self.font_combo.currentData() or self.config.font_path,
            **{key: control.value() for key, control in self.controls.items()},
        )

    def schedule_preview(self, *_):
        if not self._loading:
            self.render_timer.start()

    def update_preview(self):
        self.render_timer.stop()
        try:
            config = self.current_config()
            image = render_image(config)
        except (OSError, ValueError) as exc:
            self.statusBar().showMessage(f"预览失败：{exc}")
            return False
        self.config, self.image = config, image
        self.view.set_image(image)
        self.view.region.setRect(config.x, config.y, config.rect_width, config.rect_height)
        self.canvas_size.setText(f"{image.width()} × {image.height()} px")
        for key, extent in (
            ("x", image.width()),
            ("y", image.height()),
            ("rect_width", image.width()),
            ("rect_height", image.height()),
        ):
            self.sliders[key].setMaximum(max(extent, self.controls[key].value()))
        self.statusBar().showMessage("就绪 · 预览已更新")
        return True

    def use_config(self, config):
        try:
            render_image(config)
        except (OSError, ValueError) as exc:
            QMessageBox.warning(self, "无法加载", str(exc))
            return
        self.apply_config(config)
        self.update_preview()
        self.view.fit_page()

    def choose_background(self):
        path, _ = QFileDialog.getOpenFileName(self, "选择背景", "", IMAGE_FILTER)
        if path:
            self.use_config(replace(self.current_config(), bg_path=path))

    def create_background(self):
        dialog = ImageCreator(self)
        dialog.created.connect(
            lambda path: self.use_config(replace(self.current_config(), bg_path=path))
        )
        dialog.exec()

    def choose_font(self):
        path, _ = QFileDialog.getOpenFileName(self, "选择字体", "", "字体 (*.ttf *.otf *.ttc)")
        if path:
            self.use_config(replace(self.current_config(), font_path=path))

    def choose_color(self):
        color = QColorDialog.getColor(QColor(self.config.font_color), self)
        if color.isValid():
            self.config = replace(self.current_config(), font_color=color.name())
            self.update_ink()
            self.schedule_preview()

    def open_preset(self):
        path, _ = QFileDialog.getOpenFileName(self, "打开预设", "", "JSON (*.json)")
        if path:
            try:
                self.use_config(load_preset(path))
            except (OSError, ValueError) as exc:
                QMessageBox.warning(self, "无法打开预设", str(exc))

    def write_preset(self):
        if not self.update_preview():
            return
        path = choose_save_path(self, "保存预设", "星笺.json", "JSON (*.json)")
        if path:
            try:
                save_preset(self.config, path)
                self.statusBar().showMessage(f"预设已保存：{path}")
            except (OSError, ValueError) as exc:
                QMessageBox.warning(self, "无法保存预设", str(exc))

    def export_image(self):
        if not self.update_preview():
            QMessageBox.warning(self, "无法导出", self.statusBar().currentMessage())
            return
        path = choose_save_path(self, "导出图片", "星笺.png", SAVE_FILTER)
        if path:
            try:
                save_image(self.image, path, overwrite=True)
                self.statusBar().showMessage(f"图片已保存：{path}")
            except (OSError, ValueError) as exc:
                QMessageBox.warning(self, "导出失败", str(exc))


if __name__ == "__main__":
    app = QApplication(sys.argv)
    app.setApplicationName("星笺")
    app.setApplicationDisplayName("星笺")
    viewer = HandFontWindow()
    viewer.show()
    sys.exit(app.exec())
