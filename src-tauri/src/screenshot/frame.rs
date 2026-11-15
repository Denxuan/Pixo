use super::error::ScreenshotError;

pub fn encode(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, ScreenshotError> {
    let stride = width
        .checked_mul(4)
        .ok_or_else(|| ScreenshotError::new("invalid_input"))?;
    let pixels = (stride as usize)
        .checked_mul(height as usize)
        .ok_or_else(|| ScreenshotError::new("invalid_input"))?;
    if rgba.len() != pixels {
        return Err(ScreenshotError::new("invalid_input"));
    }
    let mut bytes = Vec::with_capacity(16 + pixels);
    bytes.extend_from_slice(b"PXFR");
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    bytes.extend_from_slice(&stride.to_le_bytes());
    bytes.extend_from_slice(rgba);
    Ok(bytes)
}

pub fn header(bytes: &[u8]) -> Result<(u32, u32, u32), ScreenshotError> {
    if bytes.len() < 16 || &bytes[0..4] != b"PXFR" {
        return Err(ScreenshotError::new("capture_failed"));
    }
    let width = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    let height = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    let stride = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
    let pixels = (stride as usize)
        .checked_mul(height as usize)
        .ok_or_else(|| ScreenshotError::new("capture_failed"))?;
    if stride != width.saturating_mul(4) || bytes.len() != 16 + pixels {
        return Err(ScreenshotError::new("capture_failed"));
    }
    Ok((width, height, stride))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_roundtrip_keeps_size_and_pixels() {
        let rgba = vec![1, 2, 3, 4, 5, 6, 7, 8];
        let bytes = encode(2, 1, &rgba).unwrap();
        assert_eq!(header(&bytes).unwrap(), (2, 1, 8));
        assert_eq!(&bytes[16..], rgba.as_slice());
    }

    #[test]
    fn frame_rejects_a_short_buffer() {
        assert!(header(b"PXFR").is_err());
    }
}
