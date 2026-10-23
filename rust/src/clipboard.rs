//! Native image clipboard. Keep unpremultiplied export pixels and report OS errors.
use anyhow::{Context, Result, ensure};
use image::RgbaImage;
use std::borrow::Cow;

#[derive(Default)]
pub struct ImageClipboard {
    // Retain the handle: clipboard ownership can depend on its lifetime on Linux.
    inner: Option<arboard::Clipboard>,
}

impl ImageClipboard {
    pub fn copy_image(&mut self, image: &RgbaImage) -> Result<()> {
        ensure!(image.width() > 0 && image.height() > 0, "无法复制空图片");
        if self.inner.is_none() {
            self.inner = Some(arboard::Clipboard::new().context("无法打开系统剪贴板")?);
        }
        self.inner
            .as_mut()
            .unwrap()
            .set_image(arboard::ImageData {
                width: image.width() as usize,
                height: image.height() as usize,
                bytes: Cow::Borrowed(image.as_raw()),
            })
            .context("无法将图片写入系统剪贴板")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_image_is_rejected_without_touching_the_clipboard() {
        let mut clipboard = ImageClipboard::default();
        assert!(clipboard.copy_image(&RgbaImage::new(0, 0)).is_err());
        assert!(clipboard.inner.is_none());
    }

    /// Run after clicking Copy Image in the native editor. This reads, never changes, the clipboard.
    #[test]
    #[ignore = "Requires a native GUI copy and XINGJIAN_CLIPBOARD_EXPECTED pointing to its PNG export"]
    fn native_clipboard_matches_export() {
        let path = std::env::var("XINGJIAN_CLIPBOARD_EXPECTED")
            .expect("Set XINGJIAN_CLIPBOARD_EXPECTED to the matching PNG export");
        let expected = image::open(path).unwrap().into_rgba8();
        let actual = arboard::Clipboard::new().unwrap().get_image().unwrap();
        assert_eq!(actual.width, expected.width() as usize);
        assert_eq!(actual.height, expected.height() as usize);
        assert_eq!(actual.bytes.as_ref(), expected.as_raw().as_slice());
    }
}
