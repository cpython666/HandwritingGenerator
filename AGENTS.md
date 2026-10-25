# Project notes

- Pure Rust native version lives in `rust/`; read `rust/README.md` for its run/build commands. Default build uses egui/eframe with Glow; do not introduce a WebView/JavaScript frontend. Keep all 9 complete fonts; `build.rs` losslessly compresses and embeds them.
- Rust GUI/CLI/MCP share `rust/src/rendering.rs`. Preserve native background pixels, clipping, literal text, and current-config exports; Rust/Qt rasterizers need not match pixel-for-pixel. Builtin resource IDs are independent of cwd. Tests: `cargo test --manifest-path rust/Cargo.toml --locked`; lint: `cargo clippy --manifest-path rust/Cargo.toml --locked --all-targets -- -D warnings`; format: `cargo fmt --manifest-path rust/Cargo.toml --check`.

- `main.py`: PyQt6 desktop editor; `ui_style.py` and `icons/`: shared platform theme. `rendering.py`: text layout, validation, presets and atomic image export. `cli.py` / `mcp_server.py`: thin adapters.
- Install: `uv sync`; optional local MCP: `uv sync --extra mcp`. Python 3.10+; `uv.lock` records tested versions.
- Run GUI: `uv run python main.py`. Headless render: `uv run python cli.py render --text '你好' --output outputs/example.png`.
- Check: `uv run --extra mcp python -m unittest discover -s tests -v`, `uv run ruff check .`, `uv run ruff format --check .`.
- Preserve literal text, background pixel dimensions, text-region clipping and GUI/CLI/MCP output parity. Keep Qt rendering on the main thread.
- Use Qt StandardKey shortcuts; ControlModifier maps to Command on macOS and Ctrl on Windows. UI display scaling must not alter exported image pixels. Both platform checks/builds are configured in `.github/workflows/check.yml`.
- Resolve bundled resources from the application directory, never the working directory. Keep `main.spec` resources aligned with the app.
- `test/` contains old local experiments; maintained regressions are in `tests/`. Generated artifacts belong in ignored `outputs/`.
- The project workflow skill is `.agents/skills/handwriting-generator/SKILL.md`.
