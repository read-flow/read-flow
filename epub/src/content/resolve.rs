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
/// Data URLs (`data:...`) are returned unchanged — they carry their own payload
/// and must not be path-joined with a base directory.
///
/// Both `base` and the return value use `/` separators (zip paths).
pub fn resolve_href(base: &str, relative: &str) -> String {
    if relative.starts_with("data:") {
        return relative.to_string();
    }

    if let Some(stripped) = relative.strip_prefix('/') {
        return stripped.to_string();
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

/// Guess MIME type from a path or data URL.
///
/// For data URLs (`data:<media-type>;...`) the media type is extracted directly
/// from the URL rather than inferred from an extension.
pub fn guess_media_type(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("data:") {
        return rest
            .split(';')
            .next()
            .unwrap_or("application/octet-stream")
            .to_string();
    }

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
    use assert4rs::Assert;
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case("OEBPS/Text/ch1.xhtml", "OEBPS/Text")]
    #[case("ch1.xhtml", "")]
    #[case("OEBPS/content.opf", "OEBPS")]
    #[case("a/b/c/d.html", "a/b/c")]
    fn test_base_dir(#[case] href: &str, #[case] expected: &str) {
        Assert::that(base_dir(href)).is(expected);
    }

    #[rstest]
    #[case("OEBPS/Text", "image.png", "OEBPS/Text/image.png")]
    #[case("OEBPS/Text", "../Images/cover.png", "OEBPS/Images/cover.png")]
    #[case("OEBPS/Text", "../../root.png", "root.png")]
    #[case("", "image.png", "image.png")]
    #[case("OEBPS", "./chapter1.xhtml", "OEBPS/chapter1.xhtml")]
    #[case("OEBPS/Text", "/absolute/path.png", "absolute/path.png")]
    #[case(
        "OEBPS/html",
        "data:image/png;base64,abc123",
        "data:image/png;base64,abc123"
    )]
    #[case("", "data:image/svg+xml;base64,xyz", "data:image/svg+xml;base64,xyz")]
    fn test_resolve_href(#[case] base: &str, #[case] relative: &str, #[case] expected: &str) {
        Assert::that(resolve_href(base, relative)).is(expected);
    }

    #[rstest]
    #[case("image.png", "image/png")]
    #[case("photo.JPG", "image/jpeg")]
    #[case("icon.gif", "image/gif")]
    #[case("drawing.svg", "image/svg+xml")]
    #[case("pic.webp", "image/webp")]
    #[case("unknown.xyz", "application/octet-stream")]
    #[case("data:image/png;base64,abc", "image/png")]
    #[case("data:image/svg+xml;base64,xyz", "image/svg+xml")]
    #[case("data:image/jpeg;base64,xxx", "image/jpeg")]
    fn test_guess_media_type(#[case] path: &str, #[case] expected: &str) {
        Assert::that(guess_media_type(path)).is(expected);
    }
}
