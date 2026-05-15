use std::path::Path;

#[derive(Debug, Clone)]
pub struct ExtractedMetadata {
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub language: Option<String>,
    pub publisher: Option<String>,
    pub identifier: Option<String>,
    pub date: Option<String>,
    pub subject: Option<String>,
}

impl ExtractedMetadata {
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.authors.is_empty()
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
        _ => None,
    }
}

fn extract_epub(path: &Path) -> Option<ExtractedMetadata> {
    use epub::Document as _;
    let doc = epub::EpubDocument::open(path).ok()?;
    let m = doc.metadata();
    Some(ExtractedMetadata {
        title: m.title.clone(),
        authors: m.authors.clone(),
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
        authors: get(mupdf::MetadataName::Author)
            .map(|a| vec![a])
            .unwrap_or_default(),
        language: None,
        publisher: None,
        identifier: None,
        date: get(mupdf::MetadataName::CreationDate).map(format_pdf_date),
        subject: get(mupdf::MetadataName::Subject),
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
    use super::*;

    #[test]
    fn unsupported_extension_returns_none() {
        assert!(extract_metadata(std::path::Path::new("/tmp/book.mobi"), "mobi").is_none());
        assert!(extract_metadata(std::path::Path::new("/tmp/book.cbz"), "cbz").is_none());
        assert!(extract_metadata(std::path::Path::new("/tmp/book.txt"), "txt").is_none());
    }

    #[test]
    fn is_empty_true_when_all_fields_empty() {
        let meta = ExtractedMetadata {
            title: None,
            authors: vec![],
            language: None,
            publisher: None,
            identifier: None,
            date: None,
            subject: None,
        };
        assert!(meta.is_empty());
    }

    #[test]
    fn is_empty_false_when_any_field_present() {
        let fields: &[fn() -> ExtractedMetadata] = &[
            || ExtractedMetadata {
                title: Some("t".into()),
                authors: vec![],
                language: None,
                publisher: None,
                identifier: None,
                date: None,
                subject: None,
            },
            || ExtractedMetadata {
                title: None,
                authors: vec!["a".into()],
                language: None,
                publisher: None,
                identifier: None,
                date: None,
                subject: None,
            },
            || ExtractedMetadata {
                title: None,
                authors: vec![],
                language: Some("en".into()),
                publisher: None,
                identifier: None,
                date: None,
                subject: None,
            },
            || ExtractedMetadata {
                title: None,
                authors: vec![],
                language: None,
                publisher: Some("p".into()),
                identifier: None,
                date: None,
                subject: None,
            },
            || ExtractedMetadata {
                title: None,
                authors: vec![],
                language: None,
                publisher: None,
                identifier: Some("id".into()),
                date: None,
                subject: None,
            },
            || ExtractedMetadata {
                title: None,
                authors: vec![],
                language: None,
                publisher: None,
                identifier: None,
                date: Some("2024".into()),
                subject: None,
            },
            || ExtractedMetadata {
                title: None,
                authors: vec![],
                language: None,
                publisher: None,
                identifier: None,
                date: None,
                subject: Some("sci-fi".into()),
            },
        ];
        for make in fields {
            assert!(!make().is_empty());
        }
    }

    #[test]
    fn format_pdf_date_converts_pdf_format() {
        assert_eq!(format_pdf_date("D:20240315120000".to_owned()), "2024-03-15");
    }

    #[test]
    fn format_pdf_date_falls_back_on_short_input() {
        assert_eq!(format_pdf_date("D:2024".to_owned()), "D:2024");
    }

    #[test]
    fn format_pdf_date_without_prefix() {
        assert_eq!(format_pdf_date("20240315".to_owned()), "2024-03-15");
    }
}
