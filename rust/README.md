# 星笺 · Rust 原生版

纯 Rust 桌面编辑器与图片渲染核心。界面使用 egui/eframe + OpenGL，字体排版使用 rustybuzz/fontdue；不需要 Python、Qt、Node、浏览器或 WebView2。完整 9 款字体和全部内置纸张随可执行文件携带，离线使用。

## 运行和构建

开发需要 Rust 1.88+；macOS 需要 Xcode Command Line Tools，Windows 需要 MSVC C++ 构建工具。首次构建下载 Rust 依赖，以后可以使用 Cargo 本地缓存。

在仓库根目录执行：

```sh
cargo run --manifest-path rust/Cargo.toml --release --locked
cargo test --manifest-path rust/Cargo.toml --locked
cargo clippy --manifest-path rust/Cargo.toml --locked --all-targets -- -D warnings
cargo fmt --manifest-path rust/Cargo.toml --check
```

macOS 打包：`sh rust/package-macos.sh`，结果在 `outputs/rust-build/星笺.app` 和同目录 ZIP。本地构建使用 ad-hoc 签名，公开分发时仍需开发者签名和公证。

Windows 构建：`cargo build --manifest-path rust/Cargo.toml --release --locked`，运行 `rust/target/release/xingjian.exe`。这个 EXE 已包含字体和纸张；无参数打开桌面，有子命令时作为 CLI/MCP 使用。GitHub Actions 的 `Rust desktop` 工作流在 macOS、Windows 分别检查和构建。界面需要正常的系统 OpenGL 驱动。

`--no-default-features` 可以构建不带桌面的 CLI/MCP。通常只需分发默认构建的一个程序。

## 桌面

左侧「内容 / 排版 / 纸张」，右侧纸张预览。支持输入和导入 UTF-8 文本、选择或导入字体、选择或导入纸张、创建纯色纸、字号/字重/墨色/字距/行距/边距/区域调整、画布缩放拖动、预设和 PNG/JPEG/WebP 导出。

直接拖动预览中的文字区域即可定位，「左边距 / 上边距」会同步更新；单击数值可以输入精确位置，也会立即移动预览文字。空格＋拖动、中键拖动或拖动文字区域外的空白可移动纸张，右键拖动用于测量。

「复制为图片」把点击时的完整排版按原尺寸写入系统剪贴板，可直接粘贴到支持图片的应用。Cmd/Ctrl + Shift + C 复制图片，Cmd/Ctrl + S 导出，Cmd/Ctrl + O 打开预设，Cmd/Ctrl + Shift + S 保存预设；普通文字复制仍使用 Cmd/Ctrl + C。

纸张和文字分层缓存，移动文字无需重新排版或上传整张图片。字号、字距等变化每隔至少 33 ms 提交一次后台预览，连续调整时仍显示已完成的结果，不必等松手。预览纹理长边最多 2048 px；复制与导出始终使用完整原始像素，显示器缩放和预览边界不进入成品。正文按纯文本处理，超出文字矩形裁切，不自动分页。

## CLI 与 MCP

以下示例使用 macOS 的构建路径；Windows 将程序名换成 `rust/target/release/xingjian.exe`：

```sh
rust/target/release/xingjian assets
rust/target/release/xingjian render --text '你好，世界！' --output outputs/rust-hello.png
rust/target/release/xingjian render --text-file letter.txt --font 'builtin:fonts/青叶手写体.ttf' --background 'builtin:bgs/letter.png' --font-size 24 --color '#24324a' --letter-spacing 0.1 --line-spacing 1.4 --output outputs/letter.png --save-preset outputs/letter.json
rust/target/release/xingjian render --preset outputs/letter.json --text '新的一封信' --output outputs/next.webp
rust/target/release/xingjian render --help
rust/target/release/xingjian mcp --output-dir outputs
```

`--text-file -` 读取标准输入。`--save-preset` 使用 `.json` 扩展名。默认不覆盖文件；`--force` 显式允许覆盖。CLI 成功输出 JSON，失败写 stderr 并返回非零状态。字号为 pt（96 DPI），位置及区域为 px，字距为附加 em，行距为倍数。

MCP 是本地 stdio 服务，提供 `list_assets` 和 `render_handwriting`，参数与 Python 版相同。仅允许在 `--output-dir` 内写出，默认不覆盖；不启动 HTTP 服务，也不上传数据。客户端配置示例：

```json
{
  "mcpServers": {
    "xingjian-rust": {
      "command": "/absolute/path/to/xingjian",
      "args": ["mcp", "--output-dir", "/absolute/path/to/outputs"]
    }
  }
}
```

## 资源、预设与效果

`build.rs` 将原始 TTF 按 Brotli 质量 11 压缩，逐字节验证解压结果后嵌入程序；运行时只解压需要使用的字体，不创建磁盘缓存。不会删字、子集化或改变字体轮廓。首次编译需要额外时间压缩字体。构建缓存和 `target/` 不属于分发包。

内置资源使用 `builtin:fonts/hand.ttf`、`builtin:bgs/letter.png` 这类稳定标识，不依赖工作目录。纯色纸使用 `solid:#RRGGBB:宽:高`，例如 `solid:#fffdf5:1240:1754`。自定义资源使用本地路径。JSON 字段与 Python 版一致，可加载资源路径仍有效的旧预设；内置标识和纯色标识是 Rust 版扩展，旧 Python 版不能直接读取这些标识。

字体文件保持完全相同，Rust 与 Qt 的字体光栅化和换行细节可能存在差异，因此不保证逐像素复制旧版。Rust 桌面、CLI 和 MCP 使用同一套实现，输出保持一致。静态字体只能模拟加粗，较轻字重保留原始轮廓。内置字体未覆盖的字符仍取决于字体本身。

边界：图片单边最多 8192 px、总计 3200 万像素、正文最多 10 万字符。当前版本面向横排中文和拉丁文；没有自动分页。原来的 Python 版继续保留在仓库根目录，用于对照。

## 代码入口

- `src/gui.rs`：原生编辑器、异步预览和导出。
- `src/clipboard.rs`：原尺寸图片复制与原生剪贴板验收。
- `src/rendering.rs`：共享排版、裁切、图片编码、校验和预设。
- `src/assets.rs` / `build.rs`：完整字体压缩与嵌入。
- `src/main.rs` / `src/mcp.rs`：CLI 和 MCP。
- `tests/cli_mcp.rs`：真实子进程的命令行、协议和输出一致性测试。

原生截图验收可使用 `XINGJIAN_SCREENSHOT=/absolute/path/ui.png`；程序等最新预览完成后截图退出。`XINGJIAN_WINDOW_SIZE=900x620` 可用于小窗口检查。截图文件只写到显式指定的位置。

## 本次验收（2026-10-24）

macOS arm64 应用实测约 28.44 MiB，ZIP 25.89 MiB，完整字体的嵌入数据约 22.49 MiB。此前 Qt 应用为 116.68 MiB、ZIP 54.03 MiB；应用体积减少约 75.6%。不同平台和编译器的结果会有差异。

19 项自动测试、Clippy 与格式检查通过。macOS 原生窗口已验证直接拖字同步坐标、输入坐标反向移动文字，以及快速拖动的最终位置。另在点击「复制为图片」后执行原生剪贴板验收，尺寸及全部 RGBA 像素与 GUI 导出的 PNG 一致。

macOS 原生界面已检查 900×620 和 840×560 小窗口；实际通过文件对话框导入中文、保存预设并导出图片，GUI 与 CLI 生成的 PNG 逐字节相同。另用 MCP 官方 Python SDK 客户端完成了 Rust 服务初始化与真实渲染调用。Windows MSVC 目标通过 `cargo check --all-targets`，尚未在 Windows 实机链接、安装和运行验收。

剪贴板验收在 GUI 复制后执行（此测试只读取剪贴板）：

```sh
XINGJIAN_CLIPBOARD_EXPECTED=/absolute/path/export.png cargo test --manifest-path rust/Cargo.toml --locked clipboard::tests::native_clipboard_matches_export -- --ignored
```

渲染基准：`cargo run --manifest-path rust/Cargo.toml --release --example render_bench -- --layers`。该命令测量 CPU 图层准备，不能直接当作界面帧率。
