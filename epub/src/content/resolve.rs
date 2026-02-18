/// Extract the directory portion of a zip path.
///
/// `"OEBPS/Text/ch1.xhtml"` → `"OEBPS/Text"`
/// `"ch1.xhtml"` → `""`
pub fn base_dir(href: &str) -> &str {
    match href.rfind('/') {
        Some(pos) => &href[..pos],
        None => "",
    }
}

/// Resolve a relative href against a base directory, handling `../` segments.
///
/// Both `base` and the return value use `/` separators (zip paths).
pub fn resolve_href(base: &str, relative: &str) -> String {
    if relative.starts_with('/') {
        return relative[1..].to_string();
    }

    let mut segments: Vec<&str> = if base.is_empty() {
        Vec::new()
    } else {
        base.split('/').collect()
    };

    for part in relative.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            other => segments.push(other),
        }
    }

    segments.join("/")
}

/// Guess MIME type from file extension.
pub fn guess_media_type(path: &str) -> String {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        _ => "application/octet-stream",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case("OEBPS/Text/ch1.xhtml", "OEBPS/Text")]
    #[case("ch1.xhtml", "")]
    #[case("OEBPS/content.opf", "OEBPS")]
    #[case("a/b/c/d.html", "a/b/c")]
    fn test_base_dir(#[case] href: &str, #[case] expected: &str) {
        assert_eq!(base_dir(href), expected);
    }

    #[rstest]
    #[case("OEBPS/Text", "image.png", "OEBPS/Text/image.png")]
    #[case("OEBPS/Text", "../Images/cover.png", "OEBPS/Images/cover.png")]
    #[case("OEBPS/Text", "../../root.png", "root.png")]
    #[case("", "image.png", "image.png")]
    #[case("OEBPS", "./chapter1.xhtml", "OEBPS/chapter1.xhtml")]
    #[case("OEBPS/Text", "/absolute/path.png", "absolute/path.png")]
    fn test_resolve_href(#[case] base: &str, #[case] relative: &str, #[case] expected: &str) {
        assert_eq!(resolve_href(base, relative), expected);
    }

    #[rstest]
    #[case("image.png", "image/png")]
    #[case("photo.JPG", "image/jpeg")]
    #[case("icon.gif", "image/gif")]
    #[case("drawing.svg", "image/svg+xml")]
    #[case("pic.webp", "image/webp")]
    #[case("unknown.xyz", "application/octet-stream")]
    fn test_guess_media_type(#[case] path: &str, #[case] expected: &str) {
        assert_eq!(guess_media_type(path), expected);
    }
}
