// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::Path;

#[derive(Debug, Clone)]
pub struct ExtractedMetadata {
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub authors: Vec<String>,
    pub description: Option<String>,
    pub language: Option<String>,
    pub publisher: Option<String>,
    pub identifier: Option<String>,
    pub date: Option<String>,
    pub subject: Option<String>,
}

impl ExtractedMetadata {
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.subtitle.is_none()
            && self.authors.is_empty()
            && self.description.is_none()
            && self.language.is_none()
            && self.publisher.is_none()
            && self.identifier.is_none()
            && self.date.is_none()
            && self.subject.is_none()
    }
}

/// Extract document metadata from a file. Returns `None` for unsupported formats
/// or when extraction fails (e.g. corrupt file).
pub fn extract_metadata(path: &Path, extension: &str) -> Option<ExtractedMetadata> {
    match extension {
        "epub" => extract_epub(path),
        "pdf" => extract_mupdf(path),
        "mobi" | "azw" | "azw3" => extract_mobi(path),
        _ => None,
    }
}

fn extract_epub(path: &Path) -> Option<ExtractedMetadata> {
    use epub::Document as _;
    let doc = epub::EpubDocument::open(path).ok()?;
    let m = doc.metadata();
    Some(ExtractedMetadata {
        title: m.title.clone(),
        subtitle: None,
        authors: m.authors.clone(),
        description: None,
        language: m.language.clone(),
        publisher: m.publisher.clone(),
        identifier: m.identifier.clone(),
        date: m.date.clone(),
        subject: None,
    })
}

fn extract_mupdf(path: &Path) -> Option<ExtractedMetadata> {
    let doc = mupdf::Document::open(path).ok()?;
    let get = |name| doc.metadata(name).ok().filter(|s: &String| !s.is_empty());
    Some(ExtractedMetadata {
        title: get(mupdf::MetadataName::Title),
        subtitle: None,
        authors: get(mupdf::MetadataName::Author)
            .map(|a| vec![a])
            .unwrap_or_default(),
        description: None,
        language: None,
        publisher: None,
        identifier: None,
        date: get(mupdf::MetadataName::CreationDate).map(format_pdf_date),
        subject: get(mupdf::MetadataName::Subject),
    })
}

fn extract_mobi(path: &Path) -> Option<ExtractedMetadata> {
    use mobi::headers::Language;
    let book = mobi::Mobi::from_path(path).ok()?;
    let nonempty = |s: String| if s.is_empty() { None } else { Some(s) };
    Some(ExtractedMetadata {
        title: nonempty(book.title()),
        subtitle: None,
        authors: book
            .author()
            .and_then(nonempty)
            .map(|a| vec![a])
            .unwrap_or_default(),
        description: None,
        language: match book.language() {
            Language::Neutral | Language::Unknown => None,
            lang => Some(format!("{lang:?}")),
        },
        publisher: book.publisher().and_then(nonempty),
        identifier: book.isbn().and_then(nonempty),
        date: book.publish_date().and_then(nonempty),
        subject: book.description().and_then(nonempty),
    })
}

/// PDF dates use the format `D:YYYYMMDDHHmmSS±HH'mm'` (PDF ref §3.8.3).
/// We display just the date portion as YYYY-MM-DD, falling back to the raw string.
pub fn format_pdf_date(raw: String) -> String {
    let digits = raw.strip_prefix("D:").unwrap_or(&raw);
    if digits.len() >= 8 {
        format!("{}-{}-{}", &digits[0..4], &digits[4..6], &digits[6..8])
    } else {
        raw
    }
}

#[cfg(test)]
mod tests {
    use assert4rs::Assert;

    use super::*;

    #[test]
    fn unsupported_extension_returns_none() {
        assert!(extract_metadata(std::path::Path::new("/tmp/book.cbz"), "cbz").is_none());
        assert!(extract_metadata(std::path::Path::new("/tmp/book.txt"), "txt").is_none());
    }

    #[test]
    fn mobi_returns_none_for_missing_file() {
        assert!(extract_metadata(std::path::Path::new("/tmp/nonexistent.mobi"), "mobi").is_none());
    }

    #[test]
    fn azw_returns_none_for_missing_file() {
        assert!(extract_metadata(std::path::Path::new("/tmp/nonexistent.azw"), "azw").is_none());
        assert!(extract_metadata(std::path::Path::new("/tmp/nonexistent.azw3"), "azw3").is_none());
    }

    fn empty_meta() -> ExtractedMetadata {
        ExtractedMetadata {
            title: None,
            subtitle: None,
            authors: vec![],
            description: None,
            language: None,
            publisher: None,
            identifier: None,
            date: None,
            subject: None,
        }
    }

    #[test]
    fn is_empty_true_when_all_fields_empty() {
        assert!(empty_meta().is_empty());
    }

    #[test]
    fn is_empty_false_when_any_field_present() {
        let fields: &[fn() -> ExtractedMetadata] = &[
            || ExtractedMetadata {
                title: Some("t".into()),
                ..empty_meta()
            },
            || ExtractedMetadata {
                subtitle: Some("s".into()),
                ..empty_meta()
            },
            || ExtractedMetadata {
                authors: vec!["a".into()],
                ..empty_meta()
            },
            || ExtractedMetadata {
                description: Some("d".into()),
                ..empty_meta()
            },
            || ExtractedMetadata {
                language: Some("en".into()),
                ..empty_meta()
            },
            || ExtractedMetadata {
                publisher: Some("p".into()),
                ..empty_meta()
            },
            || ExtractedMetadata {
                identifier: Some("id".into()),
                ..empty_meta()
            },
            || ExtractedMetadata {
                date: Some("2024".into()),
                ..empty_meta()
            },
            || ExtractedMetadata {
                subject: Some("sci-fi".into()),
                ..empty_meta()
            },
        ];
        for make in fields {
            assert!(!make().is_empty());
        }
    }

    #[test]
    fn format_pdf_date_converts_pdf_format() {
        Assert::that(format_pdf_date("D:20240315120000".to_owned())).is("2024-03-15");
    }

    #[test]
    fn format_pdf_date_falls_back_on_short_input() {
        Assert::that(format_pdf_date("D:2024".to_owned())).is("D:2024");
    }

    #[test]
    fn format_pdf_date_without_prefix() {
        Assert::that(format_pdf_date("20240315".to_owned())).is("2024-03-15");
    }
}
