use image::ImageEncoder;

use super::error::ScreenshotError;

pub fn decode_base64(input: &str) -> Result<Vec<u8>, ScreenshotError> {
    fn value(byte: u8) -> Option<u8> {
        match byte {
            b'A'..=b'Z' => Some(byte - b'A'),
            b'a'..=b'z' => Some(byte - b'a' + 26),
            b'0'..=b'9' => Some(byte - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }

    let cleaned: Vec<u8> = input
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    if cleaned.len() % 4 != 0 {
        return Err(ScreenshotError::new("invalid_input"));
    }
    let mut output = Vec::with_capacity(cleaned.len() / 4 * 3);
    for chunk in cleaned.chunks_exact(4) {
        let padding = chunk.iter().filter(|byte| **byte == b'=').count();
        if padding > 2 || chunk[..4 - padding].contains(&b'=') {
            return Err(ScreenshotError::new("invalid_input"));
        }
        let mut bits = 0u32;
        for (index, byte) in chunk.iter().enumerate() {
            if *byte == b'=' {
                continue;
            }
            let value = value(*byte).ok_or_else(|| ScreenshotError::new("invalid_input"))?;
            bits |= u32::from(value) << (18 - index * 6);
        }
        output.push((bits >> 16) as u8);
        if padding < 2 {
            output.push((bits >> 8) as u8);
        }
        if padding < 1 {
            output.push(bits as u8);
        }
    }
    Ok(output)
}

pub fn image_size(bytes: &[u8]) -> Result<(u32, u32), ScreenshotError> {
    if let Some(size) = png_size(bytes) {
        return Ok(size);
    }
    image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| ScreenshotError::new("invalid_input"))?
        .into_dimensions()
        .map_err(|_| ScreenshotError::new("invalid_input"))
}

fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24 || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let height = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    (width > 0 && height > 0).then_some((width, height))
}

/// Shrinks an image to the pin frame. A PNG that already fits is returned unchanged.
pub fn fit_pin_png(
    bytes: Vec<u8>,
    max_width: u32,
    max_height: u32,
) -> Result<Vec<u8>, ScreenshotError> {
    let (width, height) = image_size(&bytes)?;
    if width > 16_384 || height > 16_384 {
        return Err(ScreenshotError::new("too_large"));
    }
    let max_width = max_width.max(1);
    let max_height = max_height.max(1);
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") && width <= max_width && height <= max_height {
        return Ok(bytes);
    }
    let image = image::load_from_memory(&bytes).map_err(|error| {
        eprintln!("screenshot pin image: {error}");
        ScreenshotError::new("invalid_input")
    })?;
    let scale = (f64::from(max_width) / f64::from(width.max(1)))
        .min(f64::from(max_height) / f64::from(height.max(1)))
        .min(1.0);
    let target_width = (f64::from(width) * scale).round().max(1.0) as u32;
    let target_height = (f64::from(height) * scale).round().max(1.0) as u32;
    let fitted = if target_width == image.width() && target_height == image.height() {
        image
    } else {
        image.thumbnail(target_width, target_height)
    };
    let rgba = fitted.to_rgba8();
    encode_png(rgba.width(), rgba.height(), rgba.as_raw())
}

pub fn decode_rgba_png(bytes: &[u8]) -> Result<(u32, u32, Vec<u8>), ScreenshotError> {
    let image = image::load_from_memory(bytes).map_err(|error| {
        eprintln!("screenshot png: {error}");
        ScreenshotError::new("invalid_input")
    })?;
    let rgba = image.to_rgba8();
    Ok((rgba.width(), rgba.height(), rgba.into_raw()))
}

pub fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, ScreenshotError> {
    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(rgba, width, height, image::ExtendedColorType::Rgba8)
        .map_err(|error| {
            eprintln!("screenshot png: {error}");
            ScreenshotError::new("storage")
        })?;
    Ok(bytes)
}

pub fn encode_jpeg(
    width: u32,
    height: u32,
    rgba: &[u8],
    quality: u8,
) -> Result<Vec<u8>, ScreenshotError> {
    let mut rgb = Vec::with_capacity(rgba.len() / 4 * 3);
    for pixel in rgba.chunks_exact(4) {
        let alpha = f32::from(pixel[3]) / 255.0;
        for channel in 0..3 {
            let value = f32::from(pixel[channel]) * alpha + 255.0 * (1.0 - alpha);
            rgb.push(value.round().clamp(0.0, 255.0) as u8);
        }
    }
    let mut bytes = Vec::new();
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, quality);
    encoder
        .write_image(&rgb, width, height, image::ExtendedColorType::Rgb8)
        .map_err(|error| {
            eprintln!("screenshot jpeg: {error}");
            ScreenshotError::new("storage")
        })?;
    Ok(bytes)
}

pub fn preview_jpeg(encoded: &[u8], max_edge: u32) -> Result<Vec<u8>, ScreenshotError> {
    if encoded.len() > 40 * 1024 * 1024 {
        return Err(ScreenshotError::new("too_large"));
    }
    let image =
        image::load_from_memory(encoded).map_err(|_| ScreenshotError::new("invalid_input"))?;
    let edge = max_edge.clamp(32, 480);
    let thumb = image.thumbnail(edge, edge);
    let rgba = thumb.to_rgba8();
    encode_jpeg(rgba.width(), rgba.height(), rgba.as_raw(), 70)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_standard_base64() {
        assert_eq!(decode_base64("TWFu").unwrap(), b"Man");
        assert_eq!(decode_base64("TWE=").unwrap(), b"Ma");
        assert_eq!(decode_base64("TQ==").unwrap(), b"M");
    }

    #[test]
    fn a_small_png_is_pinned_unchanged_and_a_wide_one_shrinks() {
        let small = encode_png(2, 2, &[255, 0, 0, 255].repeat(4)).unwrap();
        assert_eq!(fit_pin_png(small.clone(), 8, 8).unwrap(), small);
        let wide = encode_png(40, 10, &[0, 0, 255, 255].repeat(400)).unwrap();
        let fitted = fit_pin_png(wide, 8, 8).unwrap();
        assert_eq!(image_size(&fitted).unwrap(), (8, 2));
    }

    #[test]
    fn shrinks_a_png_into_a_jpeg_preview() {
        let png = encode_png(8, 4, &[255, 0, 0, 255].repeat(32)).unwrap();
        let jpeg = preview_jpeg(&png, 4).unwrap();
        assert!(jpeg.starts_with(&[0xFF, 0xD8]));
    }
}
