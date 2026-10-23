use anyhow::{Context, Result, bail, ensure};
use std::{fs::File, io::Read, path::Path};

include!(concat!(env!("OUT_DIR"), "/embedded_assets.rs"));

pub fn list_assets() -> serde_json::Value {
    serde_json::json!({
        "fonts": FONT_NAMES.iter().map(|name| format!("builtin:fonts/{name}")).collect::<Vec<_>>(),
        "backgrounds": BACKGROUND_NAMES.iter().map(|name| format!("builtin:bgs/{name}")).collect::<Vec<_>>()
    })
}

fn read_limited(path: &str, limit: u64) -> Result<Vec<u8>> {
    let file = File::open(Path::new(path)).with_context(|| format!("无法打开文件：{path}"))?;
    ensure!(file.metadata()?.len() <= limit, "文件过大：{path}");
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= limit, "文件过大：{path}");
    Ok(bytes)
}

pub fn font_bytes(path: &str) -> Result<Vec<u8>> {
    if let Some(name) = path.strip_prefix("builtin:fonts/") {
        let (compressed, size) =
            embedded_font(name).with_context(|| format!("未知内置字体：{name}"))?;
        let mut bytes = Vec::with_capacity(size);
        brotli_decompressor::Decompressor::new(compressed, 4096)
            .take(size as u64 + 1)
            .read_to_end(&mut bytes)?;
        ensure!(bytes.len() == size, "内置字体解压失败：{name}");
        Ok(bytes)
    } else if path.starts_with("builtin:") {
        bail!("无效的内置字体：{path}")
    } else {
        read_limited(path, 64 * 1024 * 1024)
    }
}

pub fn background_bytes(path: &str) -> Result<Vec<u8>> {
    if let Some(name) = path.strip_prefix("builtin:bgs/") {
        Ok(embedded_background(name)
            .with_context(|| format!("未知内置纸张：{name}"))?
            .to_vec())
    } else if path.starts_with("builtin:") {
        bail!("无效的内置纸张：{path}")
    } else {
        read_limited(path, 128 * 1024 * 1024)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_nine_fonts_roundtrip_byte_exactly() {
        assert_eq!(FONT_NAMES.len(), 9);
        for name in FONT_NAMES {
            let original = std::fs::read(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../fonts")
                    .join(name),
            )
            .unwrap();
            assert_eq!(
                font_bytes(&format!("builtin:fonts/{name}")).unwrap(),
                original,
                "{name}"
            );
        }
    }

    #[test]
    fn resources_are_embedded_and_unknown_names_fail() {
        assert!(
            !background_bytes("builtin:bgs/letter.png")
                .unwrap()
                .is_empty()
        );
        assert!(background_bytes("builtin:bgs/../../secret").is_err());
        assert!(font_bytes("builtin:fonts/missing.ttf").is_err());
    }
}
