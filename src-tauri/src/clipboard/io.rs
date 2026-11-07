use std::borrow::Cow;

use arboard::{Clipboard, ImageData};

use super::error::ClipboardError;
use super::model::Payload;

pub fn map_arboard(error: arboard::Error) -> ClipboardError {
    match error {
        arboard::Error::ClipboardOccupied => ClipboardError::new("busy"),
        arboard::Error::ClipboardNotSupported => ClipboardError::new("unsupported"),
        arboard::Error::ContentNotAvailable => ClipboardError::new("not_found"),
        other => {
            eprintln!("clipboard io: {other}");
            ClipboardError::new("permission_denied")
        }
    }
}

pub fn read_text_or_image_filtered(
    image: bool,
    text: bool,
) -> Result<Option<Payload>, ClipboardError> {
    let mut clipboard = Clipboard::new().map_err(map_arboard)?;
    if image {
        match clipboard.get_image() {
            Ok(image) => {
                let width = u32::try_from(image.width).unwrap_or(u32::MAX);
                let height = u32::try_from(image.height).unwrap_or(u32::MAX);
                return Ok(Some(Payload::Image {
                    width,
                    height,
                    rgba: image.bytes.into_owned(),
                }));
            }
            Err(arboard::Error::ContentNotAvailable) => {}
            Err(error) => return Err(map_arboard(error)),
        }
    }
    if !text {
        return Ok(None);
    }
    match clipboard.get_text() {
        Ok(text) if !text.is_empty() => {
            Ok(Some(Payload::Text(super::model::RichText::plain(text))))
        }
        Ok(_) | Err(arboard::Error::ContentNotAvailable) => Ok(None),
        Err(error) => Err(map_arboard(error)),
    }
}

#[cfg_attr(not(any(target_os = "macos", target_os = "windows")), allow(dead_code))]
pub fn write_text(text: &str) -> Result<(), ClipboardError> {
    let mut clipboard = Clipboard::new().map_err(map_arboard)?;
    clipboard.set_text(text).map_err(map_arboard)
}

pub fn write_image(width: u32, height: u32, rgba: &[u8]) -> Result<(), ClipboardError> {
    let mut clipboard = Clipboard::new().map_err(map_arboard)?;
    clipboard
        .set_image(ImageData {
            width: width as usize,
            height: height as usize,
            bytes: Cow::Borrowed(rgba),
        })
        .map_err(map_arboard)
}
