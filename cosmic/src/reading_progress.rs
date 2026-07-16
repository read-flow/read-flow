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

/// Merge `own_position` (this viewer's own raw position string, in
/// whatever format that viewer's own parser produces) into `existing` (the
/// previously-stored combined or legacy position, if any), preserving the
/// other viewer's position untouched. Returns the new combined string to
/// persist.
pub fn merge(existing: Option<&str>, viewer: Viewer, own_position: &str) -> String {
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

    let own_value = serde_json::from_str(own_position)
        .unwrap_or_else(|_| Value::String(own_position.to_string()));
    match viewer {
        Viewer::Epub => epub = Some(own_value),
        Viewer::MuPdf => mupdf = Some(own_value),
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
    use super::*;

    #[test]
    fn merge_then_extract_round_trips_own_position() {
        let stored = merge(None, Viewer::Epub, r#"{"cfi":"epubcfi(/6/4)"}"#);
        assert_eq!(
            extract(&stored, Viewer::Epub).as_deref(),
            Some(r#"{"cfi":"epubcfi(/6/4)"}"#)
        );
    }

    #[test]
    fn merge_preserves_the_other_viewers_position() {
        let stored = merge(None, Viewer::Epub, r#"{"cfi":"epubcfi(/6/4)"}"#);
        let stored = merge(Some(&stored), Viewer::MuPdf, r#"{"page":42}"#);

        assert_eq!(
            extract(&stored, Viewer::Epub).as_deref(),
            Some(r#"{"cfi":"epubcfi(/6/4)"}"#)
        );
        assert_eq!(
            extract(&stored, Viewer::MuPdf).as_deref(),
            Some(r#"{"page":42}"#)
        );
    }

    #[test]
    fn switching_back_and_forth_keeps_both_positions_current() {
        let stored = merge(None, Viewer::Epub, r#"{"cfi":"a"}"#);
        let stored = merge(Some(&stored), Viewer::MuPdf, r#"{"page":1}"#);
        let stored = merge(Some(&stored), Viewer::Epub, r#"{"cfi":"b"}"#);
        let stored = merge(Some(&stored), Viewer::MuPdf, r#"{"page":2}"#);

        assert_eq!(
            extract(&stored, Viewer::Epub).as_deref(),
            Some(r#"{"cfi":"b"}"#)
        );
        assert_eq!(
            extract(&stored, Viewer::MuPdf).as_deref(),
            Some(r#"{"page":2}"#)
        );
    }

    #[test]
    fn extract_returns_none_when_viewer_never_saved() {
        let stored = merge(None, Viewer::Epub, r#"{"cfi":"a"}"#);
        assert_eq!(extract(&stored, Viewer::MuPdf), None);
    }

    #[test]
    fn extract_returns_none_for_absent_or_garbage_input() {
        assert_eq!(extract("", Viewer::Epub), None);
        assert_eq!(extract("not json", Viewer::MuPdf), None);
    }

    #[test]
    fn legacy_untagged_mupdf_position_migrates_into_mupdf_slot() {
        let legacy = r#"{"page":7}"#;
        assert_eq!(extract(legacy, Viewer::MuPdf).as_deref(), Some(legacy));
        assert_eq!(extract(legacy, Viewer::Epub), None);

        let stored = merge(Some(legacy), Viewer::Epub, r#"{"cfi":"a"}"#);
        assert_eq!(extract(&stored, Viewer::MuPdf).as_deref(), Some(legacy));
        assert_eq!(
            extract(&stored, Viewer::Epub).as_deref(),
            Some(r#"{"cfi":"a"}"#)
        );
    }

    #[test]
    fn legacy_untagged_epub_cfi_position_migrates_into_epub_slot() {
        let legacy = r#"{"cfi":"epubcfi(/6/4)"}"#;
        assert_eq!(extract(legacy, Viewer::Epub).as_deref(), Some(legacy));
        assert_eq!(extract(legacy, Viewer::MuPdf), None);

        let stored = merge(Some(legacy), Viewer::MuPdf, r#"{"page":3}"#);
        assert_eq!(extract(&stored, Viewer::Epub).as_deref(), Some(legacy));
        assert_eq!(
            extract(&stored, Viewer::MuPdf).as_deref(),
            Some(r#"{"page":3}"#)
        );
    }

    #[test]
    fn legacy_untagged_epub_chapter_position_migrates_into_epub_slot() {
        let legacy = r#"{"chapter":2,"block":5}"#;
        assert_eq!(extract(legacy, Viewer::Epub).as_deref(), Some(legacy));
        assert_eq!(extract(legacy, Viewer::MuPdf), None);
    }
}
