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
    let img = render_pdf_page(path, 0, 800)?;
    encode_webp(image::DynamicImage::from(img))
}

/// Render one page of a MuPDF-backed document (PDF) to an RGB image, scaled
/// to fit within `max_dim` × `max_dim` px.
pub fn render_pdf_page(path: &Path, page_index: i32, max_dim: u32) -> Option<image::RgbImage> {
    let doc = mupdf::Document::open(path).ok()?;
    let page = doc.load_page(page_index).ok()?;
    let bounds = page.bounds().ok()?;
    let w = bounds.width();
    let h = bounds.height();
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let max_dim = max_dim as f32;
    let scale = f32::min(max_dim / w, max_dim / h).max(0.01);
    let matrix = mupdf::Matrix::new_scale(scale, scale);
    let display_list = page.to_display_list(false).ok()?;
    let pixmap = display_list
        .to_pixmap(&matrix, &mupdf::Colorspace::device_rgb(), false)
        .ok()?;
    let pw = pixmap.width();
    let ph = pixmap.height();
    let samples = pixmap.samples().to_vec();
    image::RgbImage::from_raw(pw, ph, samples)
}

/// Encode a rendered page as WebP bytes (lossy, quality 82), for a live
/// preview response or for storing as a custom cover.
pub fn encode_page_webp(img: &image::RgbImage) -> Option<Vec<u8>> {
    encode_webp(image::DynamicImage::from(img.clone()))
}

/// Return the number of pages in a MuPDF-backed document (PDF).
pub fn pdf_page_count(path: &Path) -> Option<i32> {
    let doc = mupdf::Document::open(path).ok()?;
    doc.page_count().ok()
}

/// Fixed pixel bands excluded from content-detection (and from the final
/// crop) at each edge of a rendered page — e.g. to strip a page-number
/// footer before the whitespace crop runs, so it never pulls that band back
/// in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TrimMargins {
    #[serde(default)]
    pub top: u32,
    #[serde(default)]
    pub bottom: u32,
    #[serde(default)]
    pub left: u32,
    #[serde(default)]
    pub right: u32,
}

/// Crop the surrounding whitespace off a rendered page image, leaving a
/// `padding`-px margin around the detected content. `margins` are excluded
/// from content-detection and from the final crop entirely, before `padding`
/// is applied — use them to strip a known decorative band (e.g. a page
/// number) that would otherwise widen the crop. Falls back to the image
/// cropped to just the excluded margins when the remaining region is blank
/// (or near-blank) and no content bbox is found there.
pub fn trim_whitespace(
    image: &image::RgbImage,
    padding: u32,
    margins: TrimMargins,
) -> image::RgbImage {
    const WHITE_THRESHOLD: u8 = 250;

    let (width, height) = image.dimensions();
    let is_content = |x: u32, y: u32| {
        let px = image.get_pixel(x, y);
        px.0.iter().any(|&channel| channel < WHITE_THRESHOLD)
    };

    // Clamp so opposing margins never cross and leave a degenerate region.
    let margin_left = margins.left.min(width.saturating_sub(1));
    let margin_right = margins
        .right
        .min(width.saturating_sub(1).saturating_sub(margin_left));
    let margin_top = margins.top.min(height.saturating_sub(1));
    let margin_bottom = margins
        .bottom
        .min(height.saturating_sub(1).saturating_sub(margin_top));

    let inner_min_x = margin_left;
    let inner_max_x = width - 1 - margin_right;
    let inner_min_y = margin_top;
    let inner_max_y = height - 1 - margin_bottom;

    let mut min_x = inner_max_x;
    let mut min_y = inner_max_y;
    let mut max_x = inner_min_x;
    let mut max_y = inner_min_y;
    let mut found = false;

    for y in inner_min_y..=inner_max_y {
        for x in inner_min_x..=inner_max_x {
            if is_content(x, y) {
                found = true;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }

    if !found {
        let crop_w = inner_max_x - inner_min_x + 1;
        let crop_h = inner_max_y - inner_min_y + 1;
        return image::imageops::crop_imm(image, inner_min_x, inner_min_y, crop_w, crop_h)
            .to_image();
    }

    let crop_x = min_x.saturating_sub(padding).max(inner_min_x);
    let crop_y = min_y.saturating_sub(padding).max(inner_min_y);
    let crop_max_x = (max_x + padding).min(inner_max_x);
    let crop_max_y = (max_y + padding).min(inner_max_y);
    let crop_w = crop_max_x - crop_x + 1;
    let crop_h = crop_max_y - crop_y + 1;

    image::imageops::crop_imm(image, crop_x, crop_y, crop_w, crop_h).to_image()
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

#[cfg(test)]
mod tests {
    use assert4rs::Assert;
    use image::Rgb;
    use image::RgbImage;

    use super::TrimMargins;
    use super::trim_whitespace;

    fn white_canvas(width: u32, height: u32) -> RgbImage {
        RgbImage::from_pixel(width, height, Rgb([255, 255, 255]))
    }

    #[test]
    fn trim_whitespace_crops_to_centered_content_with_padding() {
        let mut img = white_canvas(100, 100);
        for y in 40..60 {
            for x in 40..60 {
                img.put_pixel(x, y, Rgb([0, 0, 0]));
            }
        }
        let trimmed = trim_whitespace(&img, 8, TrimMargins::default());
        let (w, h) = trimmed.dimensions();
        // Content bbox is 40..=59 (20px) plus 8px padding on each side, clamped to canvas.
        Assert::that(w).is(36u32);
        Assert::that(h).is(36u32);
    }

    #[test]
    fn trim_whitespace_padding_widens_the_crop() {
        let mut img = white_canvas(100, 100);
        for y in 40..60 {
            for x in 40..60 {
                img.put_pixel(x, y, Rgb([0, 0, 0]));
            }
        }
        let trimmed = trim_whitespace(&img, 20, TrimMargins::default());
        let (w, h) = trimmed.dimensions();
        // Content bbox is 40..=59 (20px) plus 20px padding on each side.
        Assert::that(w).is(60u32);
        Assert::that(h).is(60u32);
    }

    #[test]
    fn trim_whitespace_does_not_crop_content_touching_edge() {
        let mut img = white_canvas(50, 50);
        for y in 0..10 {
            for x in 0..10 {
                img.put_pixel(x, y, Rgb([0, 0, 0]));
            }
        }
        let trimmed = trim_whitespace(&img, 8, TrimMargins::default());
        let (w, h) = trimmed.dimensions();
        // Padding clamps at the canvas edge instead of going negative/out of bounds.
        Assert::that(w).is(18u32);
        Assert::that(h).is(18u32);
    }

    #[test]
    fn trim_whitespace_bottom_margin_excludes_a_page_number_footer() {
        let mut img = white_canvas(100, 100);
        // Main content block.
        for y in 20..50 {
            for x in 20..80 {
                img.put_pixel(x, y, Rgb([0, 0, 0]));
            }
        }
        // "Page number" footer close to the bottom edge, well outside the
        // main content block.
        for y in 95..98 {
            for x in 45..55 {
                img.put_pixel(x, y, Rgb([0, 0, 0]));
            }
        }

        let without_margin = trim_whitespace(&img, 0, TrimMargins::default());
        // Without exclusion, the crop stretches all the way down to the footer.
        Assert::that(without_margin.dimensions().1).is(78u32); // 20..=97

        let with_margin = trim_whitespace(
            &img,
            0,
            TrimMargins {
                bottom: 10,
                ..Default::default()
            },
        );
        // With the bottom 10px excluded, only the main content block remains.
        Assert::that(with_margin.dimensions().1).is(30u32); // 20..=49
    }

    #[test]
    fn trim_whitespace_margins_clamp_instead_of_crossing() {
        let img = white_canvas(20, 20);
        // Margins that together exceed the canvas must not panic or invert.
        let trimmed = trim_whitespace(
            &img,
            0,
            TrimMargins {
                left: 15,
                right: 15,
                top: 0,
                bottom: 0,
            },
        );
        let (w, h) = trimmed.dimensions();
        Assert::that(w >= 1).is(true);
        Assert::that(h).is(20u32);
    }

    #[test]
    fn trim_whitespace_falls_back_to_original_on_blank_page() {
        let img = white_canvas(30, 20);
        let trimmed = trim_whitespace(&img, 8, TrimMargins::default());
        Assert::that(trimmed.dimensions()).is((30u32, 20u32));
        Assert::that(trimmed.as_raw().clone()).is(img.as_raw().clone());
    }
}
