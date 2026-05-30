use std::io::Cursor;
use std::path::Path;

/// Extract a cover image from a document file.
/// Returns JPEG bytes sized to fit within 200×300 px, or `None` if unavailable.
pub fn extract_cover(path: &Path, extension: &str) -> Option<Vec<u8>> {
    match extension {
        "epub" => extract_epub_cover(path).or_else(|| extract_mupdf_cover(path)),
        "pdf" => extract_mupdf_cover(path),
        _ => None,
    }
}

fn extract_epub_cover(path: &Path) -> Option<Vec<u8>> {
    let doc = epub::EpubDocument::open(path).ok()?;
    let raw = doc.cover_bytes()?;
    decode_resize_jpeg(&raw)
}

fn extract_mupdf_cover(path: &Path) -> Option<Vec<u8>> {
    let doc = mupdf::Document::open(path).ok()?;
    let page = doc.load_page(0).ok()?;
    let bounds = page.bounds().ok()?;
    let w = bounds.width();
    let h = bounds.height();
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let scale = f32::min(300.0 / w, 400.0 / h).max(0.01);
    let matrix = mupdf::Matrix::new_scale(scale, scale);
    let display_list = page.to_display_list(false).ok()?;
    let pixmap = display_list
        .to_pixmap(&matrix, &mupdf::Colorspace::device_rgb(), false)
        .ok()?;
    let pw = pixmap.width();
    let ph = pixmap.height();
    let samples = pixmap.samples().to_vec();
    let img = image::RgbImage::from_raw(pw, ph, samples)?;
    let trimmed = trim_whitespace(image::DynamicImage::from(img));
    encode_jpeg(trimmed)
}

fn trim_whitespace(img: image::DynamicImage) -> image::DynamicImage {
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    let threshold = 240u8;

    let is_white_row =
        |y: u32| (0..w).all(|x| rgb.get_pixel(x, y).0.iter().all(|&c| c >= threshold));
    let is_white_col =
        |x: u32| (0..h).all(|y| rgb.get_pixel(x, y).0.iter().all(|&c| c >= threshold));

    let top = (0..h).find(|&y| !is_white_row(y)).unwrap_or(0);
    let bottom = (0..h)
        .rev()
        .find(|&y| !is_white_row(y))
        .unwrap_or(h.saturating_sub(1));
    let left = (0..w).find(|&x| !is_white_col(x)).unwrap_or(0);
    let right = (0..w)
        .rev()
        .find(|&x| !is_white_col(x))
        .unwrap_or(w.saturating_sub(1));

    if top > bottom || left > right {
        return img;
    }
    img.crop_imm(left, top, right - left + 1, bottom - top + 1)
}

fn decode_resize_jpeg(raw: &[u8]) -> Option<Vec<u8>> {
    let img = image::load_from_memory(raw).ok()?;
    let trimmed = trim_whitespace(img);
    let resized = trimmed.thumbnail(200, 300);
    encode_jpeg(resized)
}

fn encode_jpeg(img: image::DynamicImage) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    img.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Jpeg)
        .ok()?;
    Some(out)
}
