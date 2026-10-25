"""Shared, local Qt rendering for the desktop app, CLI and MCP server."""

import json
import math
import os
import sys
import threading
from collections import OrderedDict
from dataclasses import asdict, dataclass, fields
from functools import lru_cache
from pathlib import Path

from PyQt6.QtCore import QIODevice, QRectF, QSaveFile
from PyQt6.QtGui import (
    QColor,
    QFont,
    QFontDatabase,
    QGuiApplication,
    QImage,
    QImageReader,
    QImageWriter,
    QPainter,
    QTextBlockFormat,
    QTextCursor,
    QTextDocument,
    QTextOption,
)

ROOT = Path(getattr(sys, "_MEIPASS", Path(__file__).resolve().parent))
MAX_PIXELS = 32_000_000
MAX_DIMENSION = 8192
MAX_TEXT_LENGTH = 100_000
_application = None
_fonts = OrderedDict()


def resource_path(relative):
    return ROOT / relative


@dataclass
class RenderConfig:
    text: str = ""
    bg_path: str = str(resource_path("bgs/letter.png"))
    font_path: str = str(resource_path("fonts/hand.ttf"))
    font_size: int = 24
    font_color: str = "#202020"
    font_weight: int = 400
    x: int = 80
    y: int = 80
    font_spacing: float = 0.1
    line_spacing: float = 1.4
    rect_width: int = 560
    rect_height: int = 850


def ensure_application(headless=False):
    """Keep Qt alive; callers must render on the main thread."""
    global _application
    if threading.current_thread() is not threading.main_thread():
        raise ValueError("Qt rendering must run on the main thread")
    if QGuiApplication.instance() is None:
        if headless:
            os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
        _application = QGuiApplication([])
    return QGuiApplication.instance()


def _validate(config):
    if not isinstance(config, RenderConfig):
        raise ValueError("Expected a RenderConfig")
    if not isinstance(config.text, str) or len(config.text) > MAX_TEXT_LENGTH:
        raise ValueError(f"Text must contain at most {MAX_TEXT_LENGTH} characters")
    ranges = {
        "font_size": (1, 200),
        "font_weight": (100, 900),
        "x": (0, MAX_DIMENSION),
        "y": (0, MAX_DIMENSION),
        "rect_width": (1, MAX_DIMENSION),
        "rect_height": (1, MAX_DIMENSION),
    }
    for name, (minimum, maximum) in ranges.items():
        value = getattr(config, name)
        if type(value) is not int or not minimum <= value <= maximum:
            raise ValueError(f"{name} must be an integer from {minimum} to {maximum}")
    for name, minimum, maximum in (
        ("font_spacing", -1, 2),
        ("line_spacing", 0.1, 5),
    ):
        value = getattr(config, name)
        if (
            type(value) not in (int, float)
            or not math.isfinite(value)
            or not minimum <= value <= maximum
        ):
            raise ValueError(f"{name} must be a number from {minimum} to {maximum}")
    if not isinstance(config.font_color, str) or not QColor(config.font_color).isValid():
        raise ValueError("Invalid font color")
    for name in ("bg_path", "font_path"):
        if not isinstance(getattr(config, name), str) or not getattr(config, name):
            raise ValueError(f"{name} must be a nonempty path")


def _file_key(path):
    path = Path(path).expanduser().resolve()
    if not path.is_file():
        raise ValueError(f"File does not exist: {path}")
    stat = path.stat()
    return str(path), stat.st_mtime_ns, stat.st_size


@lru_cache(maxsize=2)
def _background(path, modified, size):
    reader = QImageReader(path)
    reader.setAutoTransform(True)
    dimensions = reader.size()
    width, height = dimensions.width(), dimensions.height()
    if width <= 0 or height <= 0:
        raise ValueError(f"Cannot read background image: {path}")
    if width > MAX_DIMENSION or height > MAX_DIMENSION or width * height > MAX_PIXELS:
        raise ValueError(f"Background exceeds {MAX_DIMENSION}px or {MAX_PIXELS} pixels")
    image = reader.read()
    if image.isNull():
        raise ValueError(f"Cannot read background image: {reader.errorString()}")
    return image.convertToFormat(QImage.Format.Format_ARGB32_Premultiplied)


def _font_family(path):
    key = _file_key(path)
    if key not in _fonts:
        font_id = QFontDatabase.addApplicationFont(key[0])
        families = QFontDatabase.applicationFontFamilies(font_id)
        if not families:
            raise ValueError(f"Cannot load font: {path}")
        _fonts[key] = font_id, families[0]
        if len(_fonts) > 16:
            _, (old_id, _) = _fonts.popitem(last=False)
            QFontDatabase.removeApplicationFont(old_id)
    _fonts.move_to_end(key)
    return _fonts[key][1]


def _document(config):
    font = QFont(_font_family(config.font_path))
    font.setPointSize(config.font_size)
    font.setWeight(QFont.Weight(config.font_weight))
    font.setLetterSpacing(
        QFont.SpacingType.AbsoluteSpacing, config.font_spacing * config.font_size * 96 / 72
    )
    document = QTextDocument()
    document.setDocumentMargin(0)
    document.setDefaultFont(font)
    option = document.defaultTextOption()
    option.setWrapMode(QTextOption.WrapMode.WrapAtWordBoundaryOrAnywhere)
    document.setDefaultTextOption(option)
    document.setPlainText(config.text)
    cursor = QTextCursor(document)
    cursor.select(QTextCursor.SelectionType.Document)
    block = QTextBlockFormat()
    block.setLineHeight(
        config.line_spacing * 100, QTextBlockFormat.LineHeightTypes.ProportionalHeight.value
    )
    cursor.mergeBlockFormat(block)
    character = cursor.charFormat()
    character.setForeground(QColor(config.font_color))
    cursor.mergeCharFormat(character)
    document.setTextWidth(config.rect_width)
    return document


def render_image(config):
    """Render literal text clipped to its rectangle, retaining the background size."""
    _validate(config)
    ensure_application()
    image = _background(*_file_key(config.bg_path)).copy()
    image.setDotsPerMeterX(3780)
    image.setDotsPerMeterY(3780)
    document = _document(config)
    document.documentLayout().setPaintDevice(image)
    painter = QPainter(image)
    try:
        painter.setRenderHint(QPainter.RenderHint.TextAntialiasing)
        painter.translate(config.x, config.y)
        clip = QRectF(0, 0, config.rect_width, config.rect_height)
        painter.setClipRect(clip)
        document.drawContents(painter, clip)
    finally:
        painter.end()
    return image


def save_image(image, path, overwrite=False):
    """Save through QSaveFile so a failed encode never replaces the old image."""
    path = Path(path).expanduser().resolve()
    extension = path.suffix.lower().lstrip(".")
    image_format = "jpeg" if extension == "jpg" else extension
    supported = {bytes(item).decode().lower() for item in QImageWriter.supportedImageFormats()}
    if image_format not in {"png", "jpeg", "webp"} or image_format not in supported:
        raise ValueError("Use an available PNG, JPG/JPEG or WebP image format")
    if image.isNull():
        raise ValueError("Cannot save an empty image")
    if path.exists() and not overwrite:
        raise FileExistsError(f"Output already exists: {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    output = QSaveFile(str(path))
    if not output.open(QIODevice.OpenModeFlag.WriteOnly):
        raise OSError(output.errorString())
    if not image.save(output, image_format.upper()):
        output.cancelWriting()
        raise OSError(f"Cannot encode image: {path}")
    if not output.commit():
        raise OSError(output.errorString())
    return path


def load_preset(path):
    path = Path(path).expanduser().resolve()
    if path.stat().st_size > 2_000_000:
        raise ValueError("Preset is too large")
    data = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(data, dict):
        raise ValueError("Preset must be a JSON object")
    unknown = set(data) - {field.name for field in fields(RenderConfig)}
    if unknown:
        raise ValueError(f"Unknown preset fields: {', '.join(sorted(unknown))}")
    for name in ("bg_path", "font_path"):
        if name in data:
            if not isinstance(data[name], str) or not data[name]:
                raise ValueError(f"{name} must be a nonempty path")
            asset = Path(data[name]).expanduser()
            data[name] = str((asset if asset.is_absolute() else path.parent / asset).resolve())
    config = RenderConfig(**data)
    _validate(config)
    return config


def save_preset(config, path):
    _validate(config)
    path = Path(path).expanduser().resolve()
    data = asdict(config)
    for name in ("bg_path", "font_path"):
        asset = Path(data[name]).expanduser().resolve()
        try:
            data[name] = os.path.relpath(asset, path.parent)
        except ValueError:  # Different Windows drives cannot have a relative path.
            data[name] = str(asset)
    content = (json.dumps(data, ensure_ascii=False, indent=2) + "\n").encode("utf-8")
    path.parent.mkdir(parents=True, exist_ok=True)
    output = QSaveFile(str(path))
    if not output.open(QIODevice.OpenModeFlag.WriteOnly):
        raise OSError(output.errorString())
    if output.write(content) != len(content):
        output.cancelWriting()
        raise OSError(f"Cannot write preset: {path}")
    if not output.commit():
        raise OSError(output.errorString())
    return path


def list_assets():
    return {
        "fonts": [
            str(path)
            for path in sorted(resource_path("fonts").iterdir())
            if path.suffix.lower() in {".ttf", ".otf"} and path.is_file()
        ],
        "backgrounds": [
            str(path)
            for path in sorted(resource_path("bgs").iterdir())
            if path.suffix.lower() in {".png", ".jpg", ".jpeg", ".webp"} and path.is_file()
        ],
    }
