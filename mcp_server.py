"""Optional local MCP adapter. Stdout is reserved for the stdio protocol."""

import argparse
from dataclasses import fields, replace
from pathlib import Path

from mcp.server.fastmcp import FastMCP

from rendering import (
    ROOT,
    RenderConfig,
    ensure_application,
    load_preset,
    render_image,
    save_image,
)
from rendering import (
    list_assets as available_assets,
)


def create_server(output_dir):
    output_root = Path(output_dir).expanduser().resolve()
    server = FastMCP("星笺", log_level="WARNING")

    @server.tool()
    async def list_assets() -> dict:
        """List built-in font/background paths and the allowed output directory."""
        return {**available_assets(), "output_directory": str(output_root)}

    @server.tool()
    async def render_handwriting(
        text: str,
        output_path: str,
        preset_path: str | None = None,
        options: dict | None = None,
        overwrite: bool = False,
    ) -> dict:
        """Render plain text to PNG/JPG/WebP within the server's output directory.

        output_path is relative to that directory, or an absolute path inside it.
        options override a preset using RenderConfig fields: font_path, bg_path,
        font_size (pt), font_color, font_weight (100–900), x/y (pixels),
        rect_width/rect_height (pixels), font_spacing (extra em), line_spacing
        (line-height multiplier). Text beyond the rectangle is clipped.
        Existing files are preserved unless overwrite is explicitly true.
        """
        path = Path(output_path).expanduser()
        path = (path if path.is_absolute() else output_root / path).resolve()
        if not path.is_relative_to(output_root) or path == output_root:
            raise ValueError("输出路径必须位于服务器 output-dir 内")
        config = load_preset(preset_path) if preset_path else RenderConfig()
        options = options or {}
        allowed = {field.name for field in fields(RenderConfig)} - {"text"}
        unknown = options.keys() - allowed
        if unknown:
            raise ValueError(f"未知排版参数：{', '.join(sorted(unknown))}")
        config = replace(config, text=text, **options)
        # Qt stays on the server's main thread; no worker-thread GUI objects.
        ensure_application(headless=True)
        image = render_image(config)
        saved = save_image(image, path, overwrite=overwrite)
        return {"path": str(saved), "width": image.width(), "height": image.height()}

    return server


def main():
    parser = argparse.ArgumentParser(description="星笺 · 手写图片生成器的本地 stdio MCP 服务")
    parser.add_argument("--output-dir", default=ROOT / "outputs", help="允许写入图片的根目录")
    args = parser.parse_args()
    ensure_application(headless=True)
    create_server(args.output_dir).run(transport="stdio")


if __name__ == "__main__":
    main()
