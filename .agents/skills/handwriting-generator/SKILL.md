---
name: handwriting-generator
description: 使用星笺将文字或文本文件生成手写图片、调整字体与纸张排版、复用 JSON 预设；也用于修改星笺（HandwritingGenerator）的 GUI、CLI 或本地 MCP 渲染功能。
---

# 星笺 · HandwritingGenerator

让文字，带一点星光。星笺是一款本地手写图片生成器；仓库与 skill 标识继续使用 `HandwritingGenerator` / `handwriting-generator`。

本项目的 GUI、CLI 和 MCP 共用 `rendering.py`。在仓库根目录操作；从别处执行时使用 `uv --directory <仓库绝对路径> run …`。

`rust/` 目录提供纯 Rust 版本：参见 [Rust 使用说明](../../../rust/README.md)。启动 `cargo run --manifest-path rust/Cargo.toml --release --locked`，构建后使用 `rust/target/release/xingjian assets` / `render` / `mcp --output-dir outputs`；Windows 程序名加 `.exe`。参数沿用下文 CLI，但内置资源优先使用 assets 返回的 `builtin:` 标识。Rust GUI、CLI、MCP 共用 `rust/src/rendering.rs`，修改 Rust 版本时不必修改 Python 实现；完整 9 款字体禁止裁剪字库。构建时压缩并验证原始字体，运行时不需要 Python、Qt 或 WebView。Rust 的内置资源/纯色纸预设标识暂不被 Python 版识别。

## 生成图片

- `uv run python cli.py assets` 列出实际可用的字体和背景，先看资源再选路径。
- `uv run python cli.py render --text '你好，世界' --output outputs/handwriting.png` 无界面生成图片。
- 长文本优先保存为 UTF-8 文件，再传 `--text-file <路径>`，避免 shell 引号问题。
- 用 `--save-preset outputs/style.json` 保存文字与排版；下次用 `--preset outputs/style.json`，并用 `--text` 或 `--text-file` 替换文字。
- 更多参数查看 `uv run python cli.py render --help`。字号单位 pt，坐标和区域宽高单位 px，字距为附加 em，行距为倍数。
- 渲染保持背景尺寸，超出文字矩形的内容会被裁切；长文本需检查输出，调整区域、字号或拆成多张。工具不会自动分页。
- 输出默认不覆盖已有文件。预设也包含文字，作为风格复用时注意替换原文。

若当前会话已连接本项目 MCP，可调用 `list_assets` 和 `render_handwriting`；MCP 未连接时直接使用 CLI。MCP 的 `output_path` 相对于服务的输出目录，`options` 使用 `RenderConfig` 字段名，例如 `rect_width`、`font_color`。无需 API Key，文字和图像处理均在本地完成。

## 修改项目

排版、校验、资源定位和保存逻辑修改 `rendering.py`，不要分别在三个入口复制。Qt 字体注册和绘制保持在主线程。资源通过 `resource_path()` 定位，测试不能假定当前目录就是仓库。桌面测量标记只存在于预览场景，导出保存核心生成的 QImage。

界面主题在 `ui_style.py`，矢量控件图标在 `icons/`，打包时随 `main.spec` 收入。快捷键用 Qt StandardKey；`ControlModifier` 在 macOS 对应 Command、Windows 对应 Ctrl。UI 的 DPI 缩放不应改变图片导出像素。

检查命令：`uv run --extra mcp python -m unittest discover -s tests -v`、`uv run ruff check .`。修改排版或界面后实际查看生成图片；测试中的 Qt 使用 offscreen 平台。
