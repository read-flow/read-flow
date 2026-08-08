// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::Path;

/// Extract a cover image from a document file.
/// Returns WebP bytes (lossy, quality 82) sized to fit within 800×800 px, or `None` if unavailable.
pub fn extract_cover(path: &Path, extension: &str) -> Option<Vec<u8>> {
    match extension {
        "epub" => extract_epub_cover(path).or_else(|| extract_mupdf_cover(path)),
        "pdf" => extract_mupdf_cover(path),
        "mobi" | "azw" | "azw3" => extract_mobi_cover(path).or_else(|| extract_mupdf_cover(path)),
        _ => None,
    }
}

fn extract_mobi_cover(path: &Path) -> Option<Vec<u8>> {
    let book = mobi::Mobi::from_path(path).ok()?;
    let images = book.image_records();
    let raw = images.first()?.content.to_vec();
    decode_resize_webp(&raw)
}

fn extract_epub_cover(path: &Path) -> Option<Vec<u8>> {
    let doc = epub::EpubDocument::open(path).ok()?;
    let raw = doc.cover_bytes()?;
    decode_resize_webp(&raw)
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
    let scale = f32::min(800.0 / w, 800.0 / h).max(0.01);
    let matrix = mupdf::Matrix::new_scale(scale, scale);
    let display_list = page.to_display_list(false).ok()?;
    let pixmap = display_list
        .to_pixmap(&matrix, &mupdf::Colorspace::device_rgb(), false)
        .ok()?;
    let pw = pixmap.width();
    let ph = pixmap.height();
    let samples = pixmap.samples().to_vec();
    let img = image::RgbImage::from_raw(pw, ph, samples)?;
    let img = image::DynamicImage::from(img);
    encode_webp(img)
}

fn decode_resize_webp(raw: &[u8]) -> Option<Vec<u8>> {
    let img = image::load_from_memory(raw).ok()?;
    let resized = img.thumbnail(800, 800);
    encode_webp(resized)
}

fn encode_webp(img: image::DynamicImage) -> Option<Vec<u8>> {
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    Some(
        webp::Encoder::from_rgb(rgb.as_raw(), w, h)
            .encode(82.0)
            .to_vec(),
    )
}
