"""A small, platform-neutral widget theme; retain the OS font and native dialogs."""

from PyQt6.QtGui import QColor, QFontDatabase, QPalette

from rendering import resource_path

STYLE = """
QWidget { color: #292834; }
QMainWindow, QDialog, QWidget#workspace { background: #f4f3f7; }
QFrame#header, QFrame#inspector, QFrame#preview { background: #ffffff; border: 1px solid #e4e2eb; border-radius: 12px; }
QFrame#header { border: 0; border-bottom: 1px solid #e4e2eb; border-radius: 0; }
QLabel#brand { font-size: 22px; font-weight: 700; }
QLabel#subtitle, QLabel#hint, QLabel#count { color: #767181; }
QLabel#sectionTitle { font-size: 14px; font-weight: 600; }
QLabel#eyebrow { font-size: 11px; color: #827b91; }
QLabel#paperName { font-weight: 600; }
QLabel#paperThumbnail { background: #f1f0f5; border: 1px solid #e5e2ec; border-radius: 8px; padding: 12px; }
QLabel#badge { background: #eeeafa; color: #6753ba; border-radius: 7px; padding: 6px 9px; font-size: 11px; }
QPushButton, QToolButton { background: #ffffff; border: 1px solid #ddd9e6; border-radius: 7px; padding: 7px 11px; min-height: 18px; }
QPushButton:hover, QToolButton:hover { background: #f2effb; border-color: #b7abdc; }
QPushButton:pressed, QToolButton:pressed { background: #e9e2fb; }
QPushButton:checked, QToolButton:checked { background: #eee8fc; color: #6045b8; border-color: #b7a4e3; }
QPushButton:focus, QToolButton:focus { border: 2px solid #8b71ce; padding: 6px 10px; }
QPushButton#primary { background: #6550bd; color: #ffffff; border-color: #6550bd; font-weight: 600; padding-left: 22px; padding-right: 22px; }
QPushButton#primary:hover { background: #5640aa; }
QPushButton#primary:pressed { background: #493492; }
QPushButton:disabled, QToolButton:disabled { background: #f4f2f7; color: #aaa4b5; border-color: #e8e4ed; }
QTextEdit, QLineEdit, QSpinBox, QDoubleSpinBox, QComboBox { background: #faf9fc; border: 1px solid #dedbe6; border-radius: 7px; padding: 7px 9px; min-height: 18px; selection-background-color: #ded3f7; selection-color: #292834; }
QTextEdit { padding: 10px; }
QTextEdit:focus, QLineEdit:focus, QSpinBox:focus, QDoubleSpinBox:focus, QComboBox:focus { border-color: #8b71ce; background: #ffffff; }
QSpinBox, QDoubleSpinBox { padding-right: 20px; }
QComboBox { padding-right: 26px; }
QComboBox::drop-down { border: 0; width: 26px; }
QComboBox::down-arrow, QSpinBox::down-arrow, QDoubleSpinBox::down-arrow { image: url("@DOWN@"); width: 12px; height: 12px; }
QSpinBox::up-arrow, QDoubleSpinBox::up-arrow { image: url("@UP@"); width: 12px; height: 12px; }
QSpinBox::up-button, QDoubleSpinBox::up-button { border: 0; width: 19px; background: transparent; }
QSpinBox::down-button, QDoubleSpinBox::down-button { border: 0; width: 19px; background: transparent; }
QComboBox QAbstractItemView { background: #ffffff; color: #292834; selection-background-color: #eee8fc; selection-color: #4e368f; border: 1px solid #dedbe6; }
QTabWidget::pane { border: 0; background: #ffffff; }
QTabBar::tab { color: #787080; background: transparent; border-bottom: 2px solid transparent; padding: 12px 25px; }
QTabBar::tab:selected { color: #6045b8; border-bottom: 2px solid #7960c7; font-weight: 600; }
QTabBar::tab:hover { background: #f8f6fc; }
QScrollArea { border: 0; background: transparent; }
QWidget#tabPage { background: #ffffff; }
QSlider::groove:horizontal { height: 4px; background: #e7e2f0; border-radius: 2px; }
QSlider::sub-page:horizontal { background: #9c86d3; border-radius: 2px; }
QSlider::handle:horizontal { background: #ffffff; border: 2px solid #8b71ce; width: 12px; height: 12px; margin: -6px 0; border-radius: 8px; }
QSlider::handle:horizontal:hover { background: #eee8fc; }
QSplitter::handle { background: transparent; }
QSplitter::handle:hover { background: #e2dcef; }
QStatusBar { background: #f4f3f7; color: #797280; font-size: 11px; }
QStatusBar::item { border: 0; }
QToolTip { background: #ffffff; color: #292834; border: 1px solid #d8d1e6; padding: 5px; }
"""


def apply_theme(app):
    if app.property("handwritingTheme"):
        return
    app.setStyle("Fusion")
    font = QFontDatabase.systemFont(QFontDatabase.SystemFont.GeneralFont)
    font.setPixelSize(13)
    app.setFont(font)
    palette = QPalette()
    for role, color in (
        (QPalette.ColorRole.Window, "#f4f3f7"),
        (QPalette.ColorRole.WindowText, "#292834"),
        (QPalette.ColorRole.Base, "#ffffff"),
        (QPalette.ColorRole.AlternateBase, "#f7f5fa"),
        (QPalette.ColorRole.Text, "#292834"),
        (QPalette.ColorRole.Button, "#ffffff"),
        (QPalette.ColorRole.ButtonText, "#292834"),
        (QPalette.ColorRole.Highlight, "#ded3f7"),
        (QPalette.ColorRole.HighlightedText, "#292834"),
        (QPalette.ColorRole.ToolTipBase, "#ffffff"),
        (QPalette.ColorRole.ToolTipText, "#292834"),
    ):
        palette.setColor(role, QColor(color))
    palette.setColor(QPalette.ColorGroup.Disabled, QPalette.ColorRole.Text, QColor("#aaa4b5"))
    app.setPalette(palette)
    app.setStyleSheet(
        STYLE.replace("@DOWN@", resource_path("icons/chevron-down.svg").as_posix()).replace(
            "@UP@", resource_path("icons/chevron-up.svg").as_posix()
        )
    )
    app.setProperty("handwritingTheme", True)
