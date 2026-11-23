//! A photo made ready to send to a mind: as it is, or no more than [`MAX_IMAGE_EDGE`] on its
//! longer side, as a `data:` URI.

use std::path::Path;

use base64::Engine as _;
use nocturne_engine::MindError;

pub const MAX_IMAGE_EDGE: u32 = 2048;

pub fn image_data_uri(path: &Path) -> Result<String, MindError> {
    let bytes = std::fs::read(path).map_err(|e| MindError::Failed(e.to_string()))?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let (bytes, mime) = match image::load_from_memory(&bytes) {
        Ok(img) if img.width().max(img.height()) > MAX_IMAGE_EDGE => {
            let small = img.resize(
                MAX_IMAGE_EDGE,
                MAX_IMAGE_EDGE,
                image::imageops::FilterType::Triangle,
            );
            let mut out = std::io::Cursor::new(Vec::new());
            let jpeg = matches!(ext.as_str(), "jpg" | "jpeg");
            let format = if jpeg {
                image::ImageFormat::Jpeg
            } else {
                image::ImageFormat::Png
            };
            let prepared = if jpeg {
                image::DynamicImage::ImageRgb8(small.to_rgb8())
            } else {
                small
            };
            prepared
                .write_to(&mut out, format)
                .map_err(|e| MindError::Failed(e.to_string()))?;
            (
                out.into_inner(),
                if jpeg { "image/jpeg" } else { "image/png" },
            )
        }
        Ok(_) => (bytes, mime_for(&ext)),
        Err(_) => match ext.as_str() {
            "gif" | "webp" | "bmp" => (bytes, mime_for(&ext)),
            _ => return Err(MindError::Unsupported(format!("image type .{ext}"))),
        },
    };
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    Ok(format!("data:{mime};base64,{b64}"))
}

fn mime_for(ext: &str) -> &'static str {
    match ext {
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        _ => "image/png",
    }
}
