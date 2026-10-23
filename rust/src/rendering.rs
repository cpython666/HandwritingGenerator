//! Shared CPU rendering for the desktop editor, CLI and MCP. All coordinates
//! refer to the exported image, independently of the desktop display scale.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    io::{Cursor, Write},
    path::{Path, PathBuf},
    time::SystemTime,
};

use anyhow::{Context, Result, bail, ensure};
use fontdue::{Font, FontSettings, Metrics};
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, Rgba, RgbaImage};
use serde::{Deserialize, Serialize};
use unicode_segmentation::UnicodeSegmentation;

use crate::assets;

pub const MAX_DIMENSION: u32 = 8192;
pub const MAX_PIXELS: u64 = 32_000_000;
pub const MAX_TEXT_LENGTH: usize = 100_000;
const MAX_INPUT_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RenderConfig {
    pub text: String,
    pub bg_path: String,
    pub font_path: String,
    pub font_size: u32,
    pub font_color: String,
    pub font_weight: u32,
    pub x: u32,
    pub y: u32,
    pub font_spacing: f32,
    pub line_spacing: f32,
    pub rect_width: u32,
    pub rect_height: u32,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            text: String::new(),
            bg_path: "builtin:bgs/letter.png".into(),
            font_path: "builtin:fonts/hand.ttf".into(),
            font_size: 24,
            font_color: "#202020".into(),
            font_weight: 400,
            x: 80,
            y: 80,
            font_spacing: 0.1,
            line_spacing: 1.4,
            rect_width: 560,
            rect_height: 850,
        }
    }
}

impl RenderConfig {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.text.chars().count() <= MAX_TEXT_LENGTH,
            "正文最多 {MAX_TEXT_LENGTH} 字符"
        );
        ensure!((1..=200).contains(&self.font_size), "字号应为 1–200 pt");
        ensure!((100..=900).contains(&self.font_weight), "字重应为 100–900");
        ensure!(
            self.x <= MAX_DIMENSION && self.y <= MAX_DIMENSION,
            "坐标不能超过 {MAX_DIMENSION} px"
        );
        ensure!(
            (1..=MAX_DIMENSION).contains(&self.rect_width)
                && (1..=MAX_DIMENSION).contains(&self.rect_height),
            "文字区域宽高应为 1–{MAX_DIMENSION} px"
        );
        ensure!(
            self.font_spacing.is_finite() && (-1.0..=2.0).contains(&self.font_spacing),
            "字距应为 -1–2 em"
        );
        ensure!(
            self.line_spacing.is_finite() && (0.1..=5.0).contains(&self.line_spacing),
            "行距应为 0.1–5 倍"
        );
        ensure!(
            !self.font_path.is_empty() && !self.bg_path.is_empty(),
            "字体和纸张不能为空"
        );
        parse_color(&self.font_color)?;
        Ok(())
    }
}

pub fn validate(config: &RenderConfig) -> Result<()> {
    config.validate()
}

/// Qt presets use #AARRGGBB for eight-digit colors. Keep that interpretation.
pub fn parse_color(value: &str) -> Result<[u8; 4]> {
    if value.len() == 9 && value.starts_with('#') {
        let value = u32::from_str_radix(&value[1..], 16).context("无效颜色")?;
        return Ok([
            (value >> 16) as u8,
            (value >> 8) as u8,
            value as u8,
            (value >> 24) as u8,
        ]);
    }
    let color: csscolorparser::Color = value
        .parse()
        .map_err(|_| anyhow::anyhow!("无效颜色：{value}"))?;
    Ok(color.to_rgba8())
}

#[derive(PartialEq)]
struct AssetKey(String, Option<SystemTime>, u64);

fn asset_key(path: &str) -> Result<AssetKey> {
    if path.starts_with("builtin:") || path.starts_with("solid:") {
        return Ok(AssetKey(path.into(), None, 0));
    }
    let path = expand_home(path);
    let metadata = fs::metadata(&path).with_context(|| format!("无法读取：{}", path.display()))?;
    ensure!(metadata.is_file(), "不是文件：{}", path.display());
    ensure!(
        metadata.len() <= MAX_INPUT_BYTES,
        "资源文件不能超过 128 MiB"
    );
    Ok(AssetKey(
        path.to_string_lossy().into_owned(),
        metadata.modified().ok(),
        metadata.len(),
    ))
}

struct LoadedFont {
    key: AssetKey,
    bytes: Vec<u8>,
    font: Font,
}

#[derive(Default)]
pub struct Renderer {
    font: Option<LoadedFont>,
    background: Option<(AssetKey, RgbaImage)>,
}

fn validate_dimensions(width: u32, height: u32) -> Result<()> {
    ensure!(
        width > 0
            && height > 0
            && width <= MAX_DIMENSION
            && height <= MAX_DIMENSION
            && u64::from(width) * u64::from(height) <= MAX_PIXELS,
        "图片单边最多 {MAX_DIMENSION} px，总计最多 {MAX_PIXELS} 像素"
    );
    Ok(())
}

fn load_background(path: &str) -> Result<RgbaImage> {
    if let Some(solid) = path.strip_prefix("solid:") {
        let parts: Vec<_> = solid.split(':').collect();
        ensure!(parts.len() == 3, "纯色纸张格式应为 solid:#RRGGBB:宽:高");
        let width = parts[1].parse::<u32>().context("无效纸张宽度")?;
        let height = parts[2].parse::<u32>().context("无效纸张高度")?;
        validate_dimensions(width, height)?;
        return Ok(RgbaImage::from_pixel(
            width,
            height,
            Rgba(parse_color(parts[0])?),
        ));
    }
    let bytes = assets::background_bytes(&expand_home(path).to_string_lossy())?;
    ensure!(bytes.len() as u64 <= MAX_INPUT_BYTES, "背景文件过大");
    let mut reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(MAX_PIXELS * 8);
    reader.limits(limits);
    let mut decoder = reader
        .into_decoder()
        .context("无法解码背景（支持 PNG、JPEG、WebP）")?;
    let (width, height) = decoder.dimensions();
    validate_dimensions(width, height)?;
    let orientation = decoder.orientation()?;
    let mut decoded = DynamicImage::from_decoder(decoder)?;
    decoded.apply_orientation(orientation);
    Ok(decoded.into_rgba8())
}

impl Renderer {
    pub fn render(&mut self, config: &RenderConfig) -> Result<RgbaImage> {
        config.validate()?;
        let bg_key = asset_key(&config.bg_path)?;
        if self
            .background
            .as_ref()
            .is_none_or(|(key, _)| *key != bg_key)
        {
            self.background = Some((bg_key, load_background(&config.bg_path)?));
        }
        let mut image = self.background.as_ref().unwrap().1.clone();
        if config.text.is_empty() {
            return Ok(image);
        }
        let font_key = asset_key(&config.font_path)?;
        if self.font.as_ref().is_none_or(|font| font.key != font_key) {
            let bytes = assets::font_bytes(&expand_home(&config.font_path).to_string_lossy())?;
            ensure!(bytes.len() as u64 <= MAX_INPUT_BYTES, "字体文件过大");
            let font = Font::from_bytes(bytes.as_slice(), FontSettings::default())
                .map_err(|error| anyhow::anyhow!("无法加载字体：{error}"))?;
            self.font = Some(LoadedFont {
                key: font_key,
                bytes,
                font,
            });
        }
        let loaded = self.font.as_ref().unwrap();
        let face = rustybuzz::Face::from_slice(&loaded.bytes, 0).context("无法读取字体轮廓")?;
        let px = config.font_size as f32 * 96.0 / 72.0;
        let metrics = loaded
            .font
            .horizontal_line_metrics(px)
            .context("字体缺少横排度量")?;
        let line_height = metrics.new_line_size * config.line_spacing;
        let color = parse_color(&config.font_color)?;
        let mut baseline = config.y as f32 + metrics.ascent;
        let bottom = config
            .y
            .saturating_add(config.rect_height)
            .min(image.height());
        let mut bitmaps = HashMap::new();
        let text = config
            .text
            .replace("\r\n", "\n")
            .replace(['\r', '\u{2028}', '\u{2029}'], "\n")
            .replace('\t', "    ");
        for paragraph in text.split('\n') {
            for line in wrap_lines(
                paragraph,
                &face,
                px,
                config.font_spacing,
                config.rect_width as f32,
            ) {
                if baseline - metrics.ascent >= bottom as f32 {
                    return Ok(image);
                }
                draw_line(
                    &mut image,
                    &loaded.font,
                    &face,
                    &line,
                    baseline,
                    config,
                    color,
                    &mut bitmaps,
                );
                baseline += line_height;
            }
        }
        Ok(image)
    }
}

fn shape(text: &str, face: &rustybuzz::Face<'_>, letter_spacing: f32) -> rustybuzz::GlyphBuffer {
    let mut buffer = rustybuzz::UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.guess_segment_properties();
    // Letter spacing applies between grapheme clusters, so discretionary
    // ligatures must not swallow their spacing slots.
    let features = if letter_spacing != 0.0 {
        vec!["liga=0".parse().unwrap(), "clig=0".parse().unwrap()]
    } else {
        Vec::new()
    };
    rustybuzz::shape(face, &features, buffer)
}

/// Wrap at Unicode word boundaries, falling back to complete grapheme clusters
/// for a word wider than the region. Whitespace remains part of the line.
fn wrap_lines(
    text: &str,
    face: &rustybuzz::Face<'_>,
    px: f32,
    spacing: f32,
    width: f32,
) -> Vec<String> {
    if text.is_empty() {
        return vec![String::new()];
    }
    let shaped = shape(text, face, spacing);
    let scale = px / face.units_per_em() as f32;
    let mut advance = BTreeMap::<usize, f32>::new();
    for (info, position) in shaped.glyph_infos().iter().zip(shaped.glyph_positions()) {
        *advance.entry(info.cluster as usize).or_default() += position.x_advance as f32 * scale;
    }
    let units: Vec<(usize, usize, f32)> = text
        .grapheme_indices(true)
        .map(|(start, grapheme)| {
            let end = start + grapheme.len();
            let advance = advance
                .range(start..end)
                .map(|(_, advance)| *advance)
                .sum::<f32>();
            (start, end, advance + spacing * px)
        })
        .collect();
    let breaks: HashSet<usize> = unicode_linebreak::linebreaks(text)
        .map(|(index, _)| index)
        .collect();
    let mut lines = Vec::new();
    let mut start = 0;
    while start < units.len() {
        let mut pen = 0.0;
        let mut cursor = start;
        let mut last_break = None;
        while cursor < units.len() {
            let next = pen + units[cursor].2;
            // Keep at least one cluster so even a very narrow region advances.
            if next - spacing * px > width && cursor > start {
                break;
            }
            pen = next;
            cursor += 1;
            if breaks.contains(&units[cursor - 1].1) {
                last_break = Some(cursor);
            }
        }
        if cursor < units.len() {
            cursor = last_break.filter(|end| *end > start).unwrap_or(cursor);
        }
        lines.push(text[units[start].0..units[cursor - 1].1].to_owned());
        start = cursor;
    }
    lines
}

type BitmapCache = HashMap<u16, (Metrics, Vec<u8>)>;

#[allow(clippy::too_many_arguments)]
fn draw_line(
    image: &mut RgbaImage,
    font: &Font,
    face: &rustybuzz::Face<'_>,
    text: &str,
    baseline: f32,
    config: &RenderConfig,
    color: [u8; 4],
    cache: &mut BitmapCache,
) {
    let px = config.font_size as f32 * 96.0 / 72.0;
    let scale = px / face.units_per_em() as f32;
    let shaped = shape(text, face, config.font_spacing);
    let mut pen = config.x as f32;
    let mut previous_cluster = None;
    let bold =
        ((config.font_weight.saturating_sub(400) as f32 / 500.0) * px * 0.035).round() as i32;
    for (info, position) in shaped.glyph_infos().iter().zip(shaped.glyph_positions()) {
        if previous_cluster.is_some_and(|cluster| cluster != info.cluster) {
            pen += config.font_spacing * px;
        }
        previous_cluster = Some(info.cluster);
        let glyph_id = info.glyph_id as u16;
        // Bound the transient glyph cache when rendering very diverse text.
        if cache.len() >= 512 && !cache.contains_key(&glyph_id) {
            cache.clear();
        }
        let (metrics, bitmap) = cache
            .entry(glyph_id)
            .or_insert_with(|| font.rasterize_indexed(glyph_id, px));
        let left = (pen + position.x_offset as f32 * scale).round() as i32 + metrics.xmin;
        let top = (baseline - position.y_offset as f32 * scale).round() as i32
            - metrics.ymin
            - metrics.height as i32;
        for row in 0..metrics.height {
            let y = top + row as i32;
            if y < config.y as i32
                || y >= (config.y + config.rect_height).min(image.height()) as i32
            {
                continue;
            }
            for column in 0..metrics.width {
                let coverage = bitmap[row * metrics.width + column];
                if coverage == 0 {
                    continue;
                }
                for shift in 0..=bold {
                    let x = left + column as i32 + shift;
                    if x < config.x as i32
                        || x >= (config.x + config.rect_width).min(image.width()) as i32
                    {
                        continue;
                    }
                    blend(image.get_pixel_mut(x as u32, y as u32), color, coverage);
                }
            }
        }
        pen += position.x_advance as f32 * scale;
    }
}

fn blend(destination: &mut Rgba<u8>, source: [u8; 4], coverage: u8) {
    let alpha = f32::from(source[3]) / 255.0 * f32::from(coverage) / 255.0;
    let old_alpha = f32::from(destination[3]) / 255.0;
    let combined = alpha + old_alpha * (1.0 - alpha);
    if combined == 0.0 {
        return;
    }
    for channel in 0..3 {
        destination[channel] = ((f32::from(source[channel]) * alpha
            + f32::from(destination[channel]) * old_alpha * (1.0 - alpha))
            / combined)
            .round() as u8;
    }
    destination[3] = (combined * 255.0).round() as u8;
}

pub fn render_image(config: &RenderConfig) -> Result<RgbaImage> {
    Renderer::default().render(config)
}

fn expand_home(value: &str) -> PathBuf {
    if value == "~" || value.starts_with("~/") || value.starts_with("~\\") {
        if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            return PathBuf::from(home).join(value.get(2..).unwrap_or(""));
        }
    }
    PathBuf::from(value)
}

fn absolute(path: &Path) -> Result<PathBuf> {
    std::path::absolute(expand_home(&path.to_string_lossy())).context("无法解析文件路径")
}

pub fn load_preset(path: &Path) -> Result<RenderConfig> {
    let path = absolute(path)?;
    ensure!(fs::metadata(&path)?.len() <= 2_000_000, "预设不能超过 2 MB");
    let mut config: RenderConfig =
        serde_json::from_slice(&fs::read(&path)?).context("无效的 JSON 预设")?;
    for value in [&mut config.font_path, &mut config.bg_path] {
        if !value.starts_with("builtin:") && !value.starts_with("solid:") {
            let asset = expand_home(value);
            *value = absolute(&path.parent().unwrap().join(asset))?
                .to_string_lossy()
                .into_owned();
        }
    }
    config.validate()?;
    Ok(config)
}

fn output_file(path: &Path, overwrite: bool) -> Result<(PathBuf, tempfile::NamedTempFile)> {
    let path = absolute(path)?;
    ensure!(path.file_name().is_some(), "输出路径需要包含文件名");
    ensure!(
        overwrite || !path.exists(),
        "文件已存在：{}（请明确允许覆盖）",
        path.display()
    );
    let parent = path.parent().context("无效输出目录")?;
    fs::create_dir_all(parent)?;
    let temporary = tempfile::NamedTempFile::new_in(parent)?;
    Ok((path, temporary))
}

fn commit_file(path: PathBuf, file: tempfile::NamedTempFile, overwrite: bool) -> Result<PathBuf> {
    file.as_file().sync_all()?;
    if overwrite {
        file.persist(&path)?;
    } else {
        file.persist_noclobber(&path)?;
    }
    Ok(path)
}

pub fn save_preset(config: &RenderConfig, path: &Path, overwrite: bool) -> Result<PathBuf> {
    config.validate()?;
    ensure!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("json")),
        "预设文件必须使用 .json 扩展名"
    );
    let (path, mut temporary) = output_file(path, overwrite)?;
    let mut saved = config.clone();
    for value in [&mut saved.font_path, &mut saved.bg_path] {
        if !value.starts_with("builtin:") && !value.starts_with("solid:") {
            let asset = absolute(Path::new(value))?;
            *value = asset
                .strip_prefix(path.parent().unwrap())
                .unwrap_or(&asset)
                .to_string_lossy()
                .into_owned();
        }
    }
    serde_json::to_writer_pretty(&mut temporary, &saved)?;
    temporary.write_all(b"\n")?;
    commit_file(path, temporary, overwrite)
}

pub fn save_image(image: &RgbaImage, path: &Path, overwrite: bool) -> Result<PathBuf> {
    validate_dimensions(image.width(), image.height())?;
    let format = match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => ImageFormat::Png,
        "jpg" | "jpeg" => ImageFormat::Jpeg,
        "webp" => ImageFormat::WebP,
        _ => bail!("图片输出支持 PNG、JPG/JPEG 和 WebP"),
    };
    let (path, mut temporary) = output_file(path, overwrite)?;
    let output = if format == ImageFormat::Jpeg {
        // JPEG has no alpha channel; flatten transparent paper onto white.
        let rgb = image::RgbImage::from_fn(image.width(), image.height(), |x, y| {
            let pixel = image.get_pixel(x, y);
            let alpha = u32::from(pixel[3]);
            image::Rgb(std::array::from_fn(|index| {
                ((u32::from(pixel[index]) * alpha + 255 * (255 - alpha) + 127) / 255) as u8
            }))
        });
        DynamicImage::ImageRgb8(rgb)
    } else {
        DynamicImage::ImageRgba8(image.clone())
    };
    output.write_to(&mut temporary, format)?;
    commit_file(path, temporary, overwrite)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> RenderConfig {
        RenderConfig {
            text: "你好，星笺！\n  Plain <text> & spaces".into(),
            bg_path: "solid:#ffffff:360:240".into(),
            x: 20,
            y: 20,
            rect_width: 180,
            rect_height: 120,
            ..Default::default()
        }
    }

    #[test]
    fn render_clips_to_region_and_preserves_paper() {
        let config = sample();
        let image = render_image(&config).unwrap();
        assert_eq!(image.dimensions(), (360, 240));
        let mut ink = 0;
        for (x, y, pixel) in image.enumerate_pixels() {
            if *pixel != Rgba([255; 4]) {
                ink += 1;
                assert!((20..200).contains(&x) && (20..140).contains(&y));
            }
        }
        assert!(ink > 100);
        let mut empty = config;
        empty.text.clear();
        assert!(
            render_image(&empty)
                .unwrap()
                .pixels()
                .all(|pixel| *pixel == Rgba([255; 4]))
        );
    }

    #[test]
    fn wrapping_preserves_all_text_and_latin_words() {
        let bytes = assets::font_bytes("builtin:fonts/hand.ttf").unwrap();
        let face = rustybuzz::Face::from_slice(&bytes, 0).unwrap();
        for text in [
            "hello world 你好世界",
            "averylongwordwithoutspaces",
            "  a <b> & c  ",
            "e\u{301}e\u{301}e\u{301}",
        ] {
            let lines = wrap_lines(text, &face, 32.0, 0.1, 30.0);
            assert_eq!(lines.concat(), text);
            assert!(lines.len() > 1);
            assert!(lines.iter().all(|line| !line.starts_with('\u{301}')));
        }
        assert_eq!(wrap_lines("", &face, 32.0, 0.1, 90.0), vec![""]);
    }

    #[test]
    fn presets_round_trip_and_reject_bad_values() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("preset.json");
        let config = sample();
        save_preset(&config, &path, false).unwrap();
        assert_eq!(load_preset(&path).unwrap(), config);
        assert!(save_preset(&config, &path, false).is_err());
        let invalid = directory.path().join("preset.png");
        assert!(save_preset(&config, &invalid, true).is_err());
        assert!(!invalid.exists());
        let mut bad = config;
        bad.line_spacing = f32::NAN;
        assert!(render_image(&bad).is_err());
        fs::write(&path, br#"{"font_size": 0}"#).unwrap();
        assert!(load_preset(&path).is_err());
        fs::write(&path, br#"{"surprise": true}"#).unwrap();
        assert!(load_preset(&path).is_err());
        assert!(load_background("solid:#fff:8192:8192").is_err());
    }

    #[test]
    fn exports_are_real_formats_and_do_not_clobber() {
        let directory = tempfile::tempdir().unwrap();
        let image = render_image(&sample()).unwrap();
        for (extension, format) in [
            ("png", ImageFormat::Png),
            ("jpg", ImageFormat::Jpeg),
            ("webp", ImageFormat::WebP),
        ] {
            let path = directory.path().join(format!("image.{extension}"));
            save_image(&image, &path, false).unwrap();
            let before = fs::read(&path).unwrap();
            assert_eq!(image::guess_format(&before).unwrap(), format);
            assert!(save_image(&image, &path, false).is_err());
            assert_eq!(fs::read(&path).unwrap(), before);
            assert_eq!(
                image::load_from_memory(&before)
                    .unwrap()
                    .into_rgba8()
                    .dimensions(),
                image.dimensions()
            );
        }
    }

    #[test]
    fn transparent_paper_and_jpeg_white_composite() {
        let config = RenderConfig {
            text: String::new(),
            bg_path: "solid:transparent:10:10".into(),
            ..Default::default()
        };
        let image = render_image(&config).unwrap();
        assert_eq!(image.get_pixel(0, 0)[3], 0);
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("white.jpg");
        save_image(&image, &path, false).unwrap();
        assert_eq!(
            image::open(path).unwrap().into_rgb8().get_pixel(0, 0).0,
            [255; 3]
        );
        assert_eq!(parse_color("#80203040").unwrap(), [32, 48, 64, 128]);
    }

    #[test]
    fn every_bundled_font_renders_and_spacing_changes_pixels() {
        let mut renderer = Renderer::default();
        let mut config = sample();
        config.text = "你好，世界！".into();
        for name in assets::FONT_NAMES {
            config.font_path = format!("builtin:fonts/{name}");
            let image = renderer.render(&config).unwrap();
            assert!(
                image.pixels().any(|pixel| *pixel != Rgba([255; 4])),
                "{name}"
            );
        }
        config.font_path = "builtin:fonts/hand.ttf".into();
        let original = renderer.render(&config).unwrap();
        config.font_spacing = 0.4;
        assert_ne!(original, renderer.render(&config).unwrap());
        config.font_spacing = 0.1;
        config.font_weight = 900;
        assert_ne!(original, renderer.render(&config).unwrap());
    }
}
