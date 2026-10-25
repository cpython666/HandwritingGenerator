# 星笺应用图标

2026-10-25 使用内置 imagegen 生成。深青绿色、奶油白纸笺和暖金色四角星延续应用的配色与「让文字，带一点星光」主题。

- `app-icon.png`：1024 px、透明外角的母版，供 macOS ICNS 打包。
- `app-icon-256.png`：256 px 运行图标，嵌入 Rust GUI，覆盖框架默认图标。
- `app-icon.ico`：同一份 256 px PNG 的 ICO 容器，用于 Windows EXE 图标。
- 根目录 `logo.png`：同一份 256 px 图标，供 Python GUI 和打包使用。

尺寸转换使用 macOS `sips`，保留透明度。图标为本仓库资源，不依赖生成工具的本机缓存路径。

## 生成提示词

```text
Use case: logo-brand.
Asset type: final production macOS and Windows desktop app icon, one square 1024 x 1024 PNG with real transparent outer corners.
Primary request: Create a distinctive, refined icon for Xingjian (星笺), a small offline handwriting-to-image app, whose identity is quiet handwritten letters with a little starlight. Deliver ONLY ONE finished icon, no mockup, no caption, no icon sheet.
Composition: front-facing flat rounded-square/squircle tile centered on a truly transparent canvas with an even modest outer inset (about 6%). Rich deep forest-jade green tile, subtly lighter toward upper left, softly shaped corners. Inside it, a generous warm ivory paper note, slightly tilted counterclockwise, with one small folded corner. The paper fills the center and remains a strong simple silhouette at 32 pixels. On the paper, two short graceful dark-jade handwritten curved strokes, abstract marks with NO readable lettering. A single distinct warm pale-gold four-point star at the upper-right corner of the paper, touching the note, evokes a spark of inspiration. The star must feel part of the composed mark. Minimal gently sculpted paper depth, restrained warm shadow, crisp contours, elegant tactile matte finish. Balanced negative space. Use the existing app's dark jade and warm cream mood.
Avoid: any letters, typography, Chinese characters, the letter e, numbers, logos from other companies, generic AI rainbow gradients, extra stars, glitter particles, galaxy background, photo, chrome/glass effects, pencils or busy tiny detail, perspective, extruded app tile, large drop shadow beyond the tile. No surrounding opaque white rectangle. True alpha transparency outside the tile.
```

