"""Generate handwriting images without opening the desktop window."""

import argparse
import json
import sys
from dataclasses import replace
from pathlib import Path

from rendering import (
    RenderConfig,
    ensure_application,
    list_assets,
    load_preset,
    render_image,
    save_image,
    save_preset,
)


def build_parser():
    parser = argparse.ArgumentParser(
        description="星笺 · 手写图片生成器（无界面）", epilog="让文字，带一点星光。"
    )
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("assets", help="以 JSON 列出内置字体和背景")
    render = commands.add_parser("render", help="生成手写图片")
    source = render.add_mutually_exclusive_group()
    source.add_argument("--text", help="直接传入文字")
    source.add_argument("--text-file", help="UTF-8 文本文件；- 表示标准输入")
    render.add_argument("--preset", help="读取 JSON 预设；命令行参数优先")
    render.add_argument("--output", required=True, help="PNG/JPG/WebP 输出路径")
    render.add_argument("--force", action="store_true", help="允许覆盖已有图片")
    render.add_argument("--save-preset", help="将本次文字与排版保存为 JSON 预设")
    render.add_argument("--font", dest="font_path", help="TTF/OTF 字体路径")
    render.add_argument("--background", dest="bg_path", help="背景图片路径")
    render.add_argument("--font-size", type=int, help="字号（pt）")
    render.add_argument("--color", dest="font_color", help="颜色名称或 #RRGGBB")
    render.add_argument("--font-weight", type=int, help="字体粗细（100–900）")
    render.add_argument("--x", type=int, help="文字区域左边距（像素）")
    render.add_argument("--y", type=int, help="文字区域上边距（像素）")
    render.add_argument("--width", dest="rect_width", type=int, help="文字区域宽度（像素）")
    render.add_argument(
        "--height", dest="rect_height", type=int, help="文字区域高度（像素，超出裁切）"
    )
    render.add_argument(
        "--letter-spacing", dest="font_spacing", type=float, help="附加字间距（em）"
    )
    render.add_argument("--line-spacing", type=float, help="行高倍数")
    return parser


def main(argv=None):
    parser = build_parser()
    args = parser.parse_args(argv)
    if args.command == "assets":
        print(json.dumps(list_assets(), ensure_ascii=False, indent=2))
        return 0
    if args.text is None and args.text_file is None and args.preset is None:
        parser.error("render 需要 --text、--text-file 或 --preset")
    try:
        config = load_preset(args.preset) if args.preset else RenderConfig()
        changes = {
            name: getattr(args, name)
            for name in RenderConfig.__dataclass_fields__
            if hasattr(args, name) and getattr(args, name) is not None
        }
        if args.text_file is not None:
            changes["text"] = (
                sys.stdin.read()
                if args.text_file == "-"
                else Path(args.text_file).read_text(encoding="utf-8-sig")
            )
        config = replace(config, **changes)
        if args.save_preset:
            preset_path = Path(args.save_preset).expanduser().resolve()
            if preset_path == Path(args.output).expanduser().resolve():
                raise ValueError("图片和预设必须使用不同的输出路径")
            if preset_path.exists() and not args.force:
                raise FileExistsError(f"预设已存在：{preset_path}（使用 --force 覆盖）")
        ensure_application(headless=True)
        image = render_image(config)
        output = save_image(image, args.output, overwrite=args.force)
        if args.save_preset:
            save_preset(config, args.save_preset)
        print(
            json.dumps(
                {"path": str(output), "width": image.width(), "height": image.height()},
                ensure_ascii=False,
            )
        )
        return 0
    except (OSError, ValueError, TypeError) as exc:
        print(f"错误：{exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
