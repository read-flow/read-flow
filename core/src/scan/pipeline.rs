// SPDX-License-Identifier: AGPL-3.0-or-later

use std::path::PathBuf;

use super::archive::SpooledFile;

/// Sent from Stage 1 (traversal) to Stage 2 (fingerprinting) over ch1.
pub struct TraversalItem {
    /// Filesystem path; the archive path when `archive_inner_path` is set.
    pub path: PathBuf,
    pub tags: Vec<String>,
    /// Path inside the archive at `path`, for archive members.
    pub archive_inner_path: Option<String>,
    /// Pre-extracted copy of the archive member (single-pass tar spooling).
    /// When set, later stages read this file instead of re-extracting.
    pub spool: Option<SpooledFile>,
}

/// Sent from Stage 2 (fingerprinting) to Stage 3 (DB writer) over ch2.
pub struct ScannedFile {
    /// Filesystem path; the archive path when `archive_inner_path` is set.
    pub path: PathBuf,
    pub extension: String,
    pub size: i64,
    pub fingerprint: String,
    pub tags: Vec<String>,
    /// Path inside the archive at `path`, for archive members.
    pub archive_inner_path: Option<String>,
    /// Pre-extracted copy of the archive member (single-pass tar spooling).
    pub spool: Option<SpooledFile>,
}

/// Progress events emitted by the scanner to the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScanProgress {
    /// Stage 1 discovered a matching file and sent it to fingerprinting.
    FileDiscovered,
    /// Stage 3 wrote (or skipped) a file to the database.
    FileProcessed {
        path: PathBuf,
        was_new: bool,
        was_updated: bool,
    },
    /// A file could not be processed; scan continues.
    FileError { path: PathBuf, error: String },
    /// All stages have finished.
    Completed {
        discovered: u64,
        processed: u64,
        errors: u64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_progress_completed_fields() {
        let ev = ScanProgress::Completed {
            discovered: 10,
            processed: 9,
            errors: 1,
        };
        let ScanProgress::Completed {
            discovered,
            processed,
            errors,
        } = ev
        else {
            panic!("wrong variant");
        };
        assert_eq!(discovered, 10);
        assert_eq!(processed, 9);
        assert_eq!(errors, 1);
    }

    #[test]
    fn scan_progress_file_processed_fields() {
        let path = PathBuf::from("/tmp/a.pdf");
        let ev = ScanProgress::FileProcessed {
            path: path.clone(),
            was_new: true,
            was_updated: false,
        };
        let ScanProgress::FileProcessed {
            path: p,
            was_new,
            was_updated,
        } = ev
        else {
            panic!("wrong variant");
        };
        assert_eq!(p, path);
        assert!(was_new);
        assert!(!was_updated);
    }

    #[test]
    fn scan_progress_file_error_fields() {
        let path = PathBuf::from("/tmp/b.pdf");
        let ev = ScanProgress::FileError {
            path: path.clone(),
            error: "permission denied".into(),
        };
        let ScanProgress::FileError { path: p, error } = ev else {
            panic!("wrong variant");
        };
        assert_eq!(p, path);
        assert_eq!(error, "permission denied");
    }
}
