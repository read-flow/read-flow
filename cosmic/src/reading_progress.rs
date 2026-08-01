// SPDX-License-Identifier: AGPL-3.0-or-later

//! Combined reading-position storage shared by the EPUB and MuPDF viewers.
//!
//! @feature: reading.progress
//!
//! The same document can be opened in either viewer (the EPUB viewer, or
//! MuPDF via the "open in another viewer" context-pane action), and each
//! keeps position in its own format: a CFI for the EPUB viewer, a page
//! number for MuPDF. Both are stored side by side in `ReadingState.position`
//! so switching viewers resumes from that viewer's own last spot instead of
//! clobbering the other one's:
//!
//! ```json
//! {"viewer": "epub", "epub": {"cfi": "..."}, "mupdf": {"page": 42}}
//! ```
//!
//! Rows written before this format existed store one viewer's raw position
//! directly, untagged. [`extract`] and [`merge`] recognize those by their
//! distinctive keys (`page` for MuPDF; `cfi`/`chapter` for the EPUB
//! viewer's own formats) so they migrate into the right slot instead of
//! being misread by the other viewer or dropped.

use serde_json::Value;
use serde_json::json;

/// Which viewer a position belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Viewer {
    Epub,
    MuPdf,
}

impl Viewer {
    fn key(self) -> &'static str {
        match self {
            Viewer::Epub => "epub",
            Viewer::MuPdf => "mupdf",
        }
    }
}

/// Which viewer an untagged (pre-combined-format) position belongs to,
/// judging by its distinctive keys. `None` if it matches neither.
fn sniff_legacy_viewer(map: &serde_json::Map<String, Value>) -> Option<Viewer> {
    if map.contains_key("page") {
        Some(Viewer::MuPdf)
    } else if map.contains_key("cfi") || map.contains_key("chapter") {
        Some(Viewer::Epub)
    } else {
        None
    }
}

/// A single viewer's own reading position, typed by format instead of
/// carried as an opaque JSON string.
///
/// TODO: the read path (`extract`, `ReadingProgressLoaded`, each viewer's
/// own progress-loading task) still hands back/consumes raw JSON strings.
/// It could follow `ViewerPosition` the same way the write path now does.
#[derive(Debug, Clone, PartialEq)]
pub enum ViewerPosition {
    Cfi(String),
    /// 0-based page index.
    Page(usize),
}

impl ViewerPosition {
    pub fn viewer(&self) -> Viewer {
        match self {
            Self::Cfi(_) => Viewer::Epub,
            Self::Page(_) => Viewer::MuPdf,
        }
    }

    /// The wire-format JSON value for this position. Page numbers are
    /// 1-based on the wire (the human-visible page number, matching the
    /// PWA's own `currentPage`), hence the `+ 1`.
    fn to_value(&self) -> Value {
        match self {
            Self::Cfi(cfi) => json!({ "cfi": cfi }),
            Self::Page(page) => json!({ "page": page + 1 }),
        }
    }
}

/// A viewer's reading position and how far through the document it is.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadingProgress {
    pub position: ViewerPosition,
    pub percentage: f64,
}

/// Extract `viewer`'s own position from a stored position string, as a raw
/// string ready to feed into that viewer's own parser. `None` means no
/// saved position for this viewer (it should start from the beginning).
pub fn extract(stored: &str, viewer: Viewer) -> Option<String> {
    let Ok(Value::Object(map)) = serde_json::from_str::<Value>(stored) else {
        return None;
    };

    if map.contains_key("viewer") {
        return map
            .get(viewer.key())
            .filter(|v| !v.is_null())
            .map(ToString::to_string);
    }

    // Untagged legacy row: only hand it back if it looks like this viewer's
    // own format, otherwise it belongs to the other viewer.
    (sniff_legacy_viewer(&map) == Some(viewer)).then(|| stored.to_string())
}

/// Merge `own_position` (this viewer's own position) into `existing` (the
/// previously-stored combined or legacy position, if any), preserving the
/// other viewer's position untouched. Returns the new combined string to
/// persist.
pub fn merge(existing: Option<&str>, own_position: &ViewerPosition) -> String {
    let mut epub: Option<Value> = None;
    let mut mupdf: Option<Value> = None;

    if let Some(Value::Object(map)) = existing.and_then(|s| serde_json::from_str(s).ok()) {
        if map.contains_key("viewer") {
            epub = map.get("epub").cloned().filter(|v| !v.is_null());
            mupdf = map.get("mupdf").cloned().filter(|v| !v.is_null());
        } else {
            match sniff_legacy_viewer(&map) {
                Some(Viewer::Epub) => epub = Some(Value::Object(map)),
                Some(Viewer::MuPdf) => mupdf = Some(Value::Object(map)),
                None => {}
            }
        }
    }

    let viewer = own_position.viewer();
    match viewer {
        Viewer::Epub => epub = Some(own_position.to_value()),
        Viewer::MuPdf => mupdf = Some(own_position.to_value()),
    }

    json!({
        "viewer": viewer.key(),
        "epub": epub,
        "mupdf": mupdf,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use assert4rs::Assert;

    use super::*;

    #[test]
    fn merge_then_extract_round_trips_own_position() {
        let stored = merge(None, &ViewerPosition::Cfi("epubcfi(/6/4)".to_string()));
        Assert::that(extract(&stored, Viewer::Epub).as_deref())
            .is_some(r#"{"cfi":"epubcfi(/6/4)"}"#);
    }

    #[test]
    fn merge_preserves_the_other_viewers_position() {
        let stored = merge(None, &ViewerPosition::Cfi("epubcfi(/6/4)".to_string()));
        // Page(41) is the 0-based active_page; the wire format is 1-based ("page":42).
        let stored = merge(Some(&stored), &ViewerPosition::Page(41));

        Assert::that(extract(&stored, Viewer::Epub).as_deref())
            .is_some(r#"{"cfi":"epubcfi(/6/4)"}"#);
        Assert::that(extract(&stored, Viewer::MuPdf).as_deref()).is_some(r#"{"page":42}"#);
    }

    #[test]
    fn switching_back_and_forth_keeps_both_positions_current() {
        let stored = merge(None, &ViewerPosition::Cfi("a".to_string()));
        let stored = merge(Some(&stored), &ViewerPosition::Page(0));
        let stored = merge(Some(&stored), &ViewerPosition::Cfi("b".to_string()));
        let stored = merge(Some(&stored), &ViewerPosition::Page(1));

        Assert::that(extract(&stored, Viewer::Epub).as_deref()).is_some(r#"{"cfi":"b"}"#);
        Assert::that(extract(&stored, Viewer::MuPdf).as_deref()).is_some(r#"{"page":2}"#);
    }

    #[test]
    fn extract_returns_none_when_viewer_never_saved() {
        let stored = merge(None, &ViewerPosition::Cfi("a".to_string()));
        Assert::that(extract(&stored, Viewer::MuPdf)).is(None);
    }

    #[test]
    fn extract_returns_none_for_absent_or_garbage_input() {
        Assert::that(extract("", Viewer::Epub)).is(None);
        Assert::that(extract("not json", Viewer::MuPdf)).is(None);
    }

    #[test]
    fn legacy_untagged_mupdf_position_migrates_into_mupdf_slot() {
        let legacy = r#"{"page":7}"#;
        Assert::that(extract(legacy, Viewer::MuPdf).as_deref()).is_some(legacy);
        Assert::that(extract(legacy, Viewer::Epub)).is(None);

        let stored = merge(Some(legacy), &ViewerPosition::Cfi("a".to_string()));
        Assert::that(extract(&stored, Viewer::MuPdf).as_deref()).is_some(legacy);
        Assert::that(extract(&stored, Viewer::Epub).as_deref()).is_some(r#"{"cfi":"a"}"#);
    }

    #[test]
    fn legacy_untagged_epub_cfi_position_migrates_into_epub_slot() {
        let legacy = r#"{"cfi":"epubcfi(/6/4)"}"#;
        Assert::that(extract(legacy, Viewer::Epub).as_deref()).is_some(legacy);
        Assert::that(extract(legacy, Viewer::MuPdf)).is(None);

        // Page(2) is 0-based; the wire format is 1-based ("page":3).
        let stored = merge(Some(legacy), &ViewerPosition::Page(2));
        Assert::that(extract(&stored, Viewer::Epub).as_deref()).is_some(legacy);
        Assert::that(extract(&stored, Viewer::MuPdf).as_deref()).is_some(r#"{"page":3}"#);
    }

    #[test]
    fn legacy_untagged_epub_chapter_position_migrates_into_epub_slot() {
        let legacy = r#"{"chapter":2,"block":5}"#;
        Assert::that(extract(legacy, Viewer::Epub).as_deref()).is_some(legacy);
        Assert::that(extract(legacy, Viewer::MuPdf)).is(None);
    }

    #[test]
    fn viewer_position_cfi_produces_the_cfi_wire_shape() {
        assert_eq!(
            ViewerPosition::Cfi("epubcfi(/6/4)".to_string()).to_value(),
            serde_json::json!({"cfi": "epubcfi(/6/4)"})
        );
    }

    #[test]
    fn viewer_position_page_produces_one_based_wire_shape() {
        assert_eq!(
            ViewerPosition::Page(0).to_value(),
            serde_json::json!({"page": 1})
        );
    }

    #[test]
    fn viewer_position_reports_its_own_viewer() {
        assert_eq!(ViewerPosition::Cfi("a".to_string()).viewer(), Viewer::Epub);
        assert_eq!(ViewerPosition::Page(0).viewer(), Viewer::MuPdf);
    }
}
