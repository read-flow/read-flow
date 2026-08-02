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
//! directly, untagged (`{"cfi": "..."}` or `{"page": 42}`). [`StoredPosition`]
//! carries both the nested and the top-level legacy fields as plain
//! `Option`s — serde already treats an absent key as `None`, so no manual
//! tagged/untagged dispatch is needed — and [`StoredPosition::epub`] /
//! [`StoredPosition::mupdf`] fall back to the legacy field when the nested
//! one is absent.

use serde::Deserialize;
use serde::Serialize;

/// Which viewer a position belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Viewer {
    Epub,
    MuPdf,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct EpubPosition {
    cfi: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct MuPdfPosition {
    /// 1-based wire page number.
    page: u64,
}

/// The on-disk shape of `ReadingState.position`. The `cfi`/`page` fields
/// only ever appear on rows written before the combined `viewer`/`epub`/
/// `mupdf` envelope existed — see the module docs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct StoredPosition {
    #[serde(skip_serializing_if = "Option::is_none")]
    viewer: Option<Viewer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    epub: Option<EpubPosition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mupdf: Option<MuPdfPosition>,
    #[deprecated(
        note = "pre-envelope legacy row shape — only read, never written; remove once no stored rows use it"
    )]
    #[serde(skip_serializing_if = "Option::is_none")]
    cfi: Option<String>,
    #[deprecated(
        note = "pre-envelope legacy row shape — only read, never written; remove once no stored rows use it"
    )]
    #[serde(skip_serializing_if = "Option::is_none")]
    page: Option<u64>,
}

impl StoredPosition {
    #[allow(
        deprecated,
        reason = "the whole point of this fallback is reading the legacy field"
    )]
    fn epub(&self) -> Option<EpubPosition> {
        self.epub
            .clone()
            .or_else(|| self.cfi.clone().map(|cfi| EpubPosition { cfi }))
    }

    #[allow(
        deprecated,
        reason = "the whole point of this fallback is reading the legacy field"
    )]
    fn mupdf(&self) -> Option<MuPdfPosition> {
        self.mupdf
            .clone()
            .or_else(|| self.page.map(|page| MuPdfPosition { page }))
    }
}

/// A single viewer's own reading position, typed by format instead of
/// carried as an opaque JSON string.
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
}

/// A viewer's reading position and how far through the document it is.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadingProgress {
    pub position: ViewerPosition,
    pub percentage: f64,
}

/// Extract `viewer`'s own position from a stored position string. `None`
/// means no saved position for this viewer (it should start from the
/// beginning).
pub fn extract(stored: &str, viewer: Viewer) -> Option<ViewerPosition> {
    let parsed: StoredPosition = serde_json::from_str(stored).ok()?;
    match viewer {
        Viewer::Epub => parsed.epub().map(|e| ViewerPosition::Cfi(e.cfi)),
        Viewer::MuPdf => parsed
            .mupdf()
            .map(|m| ViewerPosition::Page((m.page as usize).saturating_sub(1))),
    }
}

/// Merge `own_position` (this viewer's own position) into `existing` (the
/// previously-stored combined or legacy position, if any), preserving the
/// other viewer's position untouched. Returns the new combined string to
/// persist.
pub fn merge(existing: Option<&str>, own_position: &ViewerPosition) -> String {
    let existing: StoredPosition = existing
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();

    let mut epub = existing.epub();
    let mut mupdf = existing.mupdf();

    match own_position {
        ViewerPosition::Cfi(cfi) => epub = Some(EpubPosition { cfi: cfi.clone() }),
        ViewerPosition::Page(page) => {
            mupdf = Some(MuPdfPosition {
                page: (*page + 1) as u64,
            })
        }
    }

    serde_json::to_string(&StoredPosition {
        viewer: Some(own_position.viewer()),
        epub,
        mupdf,
        ..Default::default()
    })
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use assert4rs::Assert;

    use super::*;

    #[test]
    fn merge_then_extract_round_trips_own_position() {
        let stored = merge(None, &ViewerPosition::Cfi("epubcfi(/6/4)".to_string()));
        Assert::that(extract(&stored, Viewer::Epub))
            .is(Some(ViewerPosition::Cfi("epubcfi(/6/4)".to_string())));
    }

    #[test]
    fn merge_preserves_the_other_viewers_position() {
        let stored = merge(None, &ViewerPosition::Cfi("epubcfi(/6/4)".to_string()));
        let stored = merge(Some(&stored), &ViewerPosition::Page(41));

        Assert::that(extract(&stored, Viewer::Epub))
            .is(Some(ViewerPosition::Cfi("epubcfi(/6/4)".to_string())));
        Assert::that(extract(&stored, Viewer::MuPdf)).is(Some(ViewerPosition::Page(41)));
    }

    #[test]
    fn switching_back_and_forth_keeps_both_positions_current() {
        let stored = merge(None, &ViewerPosition::Cfi("a".to_string()));
        let stored = merge(Some(&stored), &ViewerPosition::Page(0));
        let stored = merge(Some(&stored), &ViewerPosition::Cfi("b".to_string()));
        let stored = merge(Some(&stored), &ViewerPosition::Page(1));

        Assert::that(extract(&stored, Viewer::Epub)).is(Some(ViewerPosition::Cfi("b".to_string())));
        Assert::that(extract(&stored, Viewer::MuPdf)).is(Some(ViewerPosition::Page(1)));
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
        // Wire "page":7 is 1-based; ViewerPosition::Page is 0-based.
        let legacy = r#"{"page":7}"#;
        Assert::that(extract(legacy, Viewer::MuPdf)).is(Some(ViewerPosition::Page(6)));
        Assert::that(extract(legacy, Viewer::Epub)).is(None);

        let stored = merge(Some(legacy), &ViewerPosition::Cfi("a".to_string()));
        Assert::that(extract(&stored, Viewer::MuPdf)).is(Some(ViewerPosition::Page(6)));
        Assert::that(extract(&stored, Viewer::Epub)).is(Some(ViewerPosition::Cfi("a".to_string())));
    }

    #[test]
    fn legacy_untagged_epub_cfi_position_migrates_into_epub_slot() {
        let legacy = r#"{"cfi":"epubcfi(/6/4)"}"#;
        Assert::that(extract(legacy, Viewer::Epub))
            .is(Some(ViewerPosition::Cfi("epubcfi(/6/4)".to_string())));
        Assert::that(extract(legacy, Viewer::MuPdf)).is(None);

        let stored = merge(Some(legacy), &ViewerPosition::Page(2));
        Assert::that(extract(&stored, Viewer::Epub))
            .is(Some(ViewerPosition::Cfi("epubcfi(/6/4)".to_string())));
        Assert::that(extract(&stored, Viewer::MuPdf)).is(Some(ViewerPosition::Page(2)));
    }

    #[test]
    fn pre_cfi_epub_chapter_block_rows_are_no_longer_recognized() {
        // Support for the pre-CFI {"chapter":N,"block":M} format was removed —
        // such rows are now indistinguishable from noise and simply ignored.
        let legacy = r#"{"chapter":2,"block":5}"#;
        Assert::that(extract(legacy, Viewer::Epub)).is(None);
        Assert::that(extract(legacy, Viewer::MuPdf)).is(None);
    }

    #[test]
    fn viewer_position_reports_its_own_viewer() {
        assert_eq!(ViewerPosition::Cfi("a".to_string()).viewer(), Viewer::Epub);
        assert_eq!(ViewerPosition::Page(0).viewer(), Viewer::MuPdf);
    }

    #[test]
    #[allow(
        deprecated,
        reason = "exercising the legacy fallback requires constructing it"
    )]
    fn stored_position_epub_falls_back_to_the_legacy_top_level_cfi_field() {
        let stored = StoredPosition {
            cfi: Some("a".to_string()),
            ..Default::default()
        };
        assert_eq!(
            stored.epub(),
            Some(EpubPosition {
                cfi: "a".to_string()
            })
        );
    }

    #[test]
    #[allow(
        deprecated,
        reason = "exercising the legacy fallback requires constructing it"
    )]
    fn stored_position_mupdf_falls_back_to_the_legacy_top_level_page_field() {
        let stored = StoredPosition {
            page: Some(7),
            ..Default::default()
        };
        assert_eq!(stored.mupdf(), Some(MuPdfPosition { page: 7 }));
    }

    #[test]
    #[allow(
        deprecated,
        reason = "exercising the legacy fallback requires constructing it"
    )]
    fn stored_position_prefers_the_nested_field_over_the_legacy_one() {
        let stored = StoredPosition {
            epub: Some(EpubPosition {
                cfi: "nested".to_string(),
            }),
            cfi: Some("legacy".to_string()),
            ..Default::default()
        };
        assert_eq!(
            stored.epub(),
            Some(EpubPosition {
                cfi: "nested".to_string()
            })
        );
    }

    #[test]
    fn viewer_serializes_as_a_lowercase_string() {
        assert_eq!(serde_json::to_string(&Viewer::Epub).unwrap(), "\"epub\"");
        assert_eq!(serde_json::to_string(&Viewer::MuPdf).unwrap(), "\"mupdf\"");
    }
}
