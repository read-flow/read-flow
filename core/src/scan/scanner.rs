use std::path::{Path, PathBuf};

use tokio::sync::mpsc;

use super::ScanSettings;
use super::pipeline::{ScanProgress, ScannedFile, TraversalItem};

const SCM_MARKERS: &[&str] = &[".git", ".hg"];

pub struct Scanner {
    settings: ScanSettings,
}

impl Scanner {
    pub fn new(settings: ScanSettings) -> Self {
        Self { settings }
    }

    /// Run the full three-stage pipeline for the given root path.
    /// Returns a receiver of progress events. The final event is always
    /// `ScanProgress::Completed`.
    pub async fn scan(
        &self,
        root: PathBuf,
        pool: sqlx::SqlitePool,
    ) -> mpsc::Receiver<ScanProgress> {
        let (progress_tx, progress_rx) = mpsc::channel(256);
        let (ch1_tx, ch1_rx) = mpsc::channel::<TraversalItem>(self.settings.concurrency * 2);
        let (ch2_tx, ch2_rx) = mpsc::channel::<ScannedFile>(64);

        let settings = self.settings.clone();
        let progress_tx2 = progress_tx.clone();
        let progress_tx3 = progress_tx.clone();

        // Stage 1 — traversal
        tokio::spawn(async move {
            stage1_traversal(root, &settings, ch1_tx, progress_tx).await;
        });

        // Stage 2 — fingerprinting
        let concurrency = self.settings.concurrency;
        tokio::spawn(async move {
            stage2_fingerprint(ch1_rx, ch2_tx, concurrency, progress_tx2).await;
        });

        // Stage 3 — DB writer
        tokio::spawn(async move {
            stage3_writer(ch2_rx, pool, progress_tx3).await;
        });

        progress_rx
    }
}

// ---------------------------------------------------------------------------
// Stage 1: traversal
// ---------------------------------------------------------------------------

async fn stage1_traversal(
    root: PathBuf,
    settings: &ScanSettings,
    tx: mpsc::Sender<TraversalItem>,
    progress_tx: mpsc::Sender<ScanProgress>,
) {
    visit_dir(&root, &root, settings, &tx, &progress_tx).await;
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with('.'))
        .unwrap_or(true)
}

fn is_scm_root(dir: &Path) -> bool {
    SCM_MARKERS.iter().any(|m| dir.join(m).is_dir())
}

fn extension_matches(path: &Path, extensions: &[String]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| extensions.iter().any(|x| x.eq_ignore_ascii_case(e)))
        .unwrap_or(false)
}

/// Collect tags for a file at `path` from the scan directory settings.
/// Returns `None` if the path falls under an `Ignore` directory.
fn tags_for_path(path: &Path, settings: &ScanSettings) -> Option<Vec<String>> {
    match settings.directory_settings_of(path) {
        Some(super::DirectorySettings::Ignore { .. }) => None,
        Some(super::DirectorySettings::Scan { tags, .. }) => Some(tags),
        None => Some(vec![]),
    }
}

fn visit_dir<'a>(
    dir: &'a Path,
    root: &'a Path,
    settings: &'a ScanSettings,
    tx: &'a mpsc::Sender<TraversalItem>,
    progress_tx: &'a mpsc::Sender<ScanProgress>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
    Box::pin(async move {
        if is_scm_root(dir) {
            tracing::debug!("skipping SCM root: {dir:?}");
            return;
        }

        let mut read_dir = match tokio::fs::read_dir(dir).await {
            Ok(rd) => rd,
            Err(e) => {
                tracing::error!("cannot read directory {dir:?}: {e}");
                return;
            }
        };

        let mut entries: Vec<PathBuf> = Vec::new();
        loop {
            match read_dir.next_entry().await {
                Ok(Some(e)) => {
                    let path = e.path();
                    if !is_hidden(&path) {
                        entries.push(path);
                    }
                }
                Ok(None) => break,
                Err(e) => tracing::error!("read_dir entry error in {dir:?}: {e}"),
            }
        }

        for path in entries {
            let meta = match tokio::fs::metadata(&path).await {
                Ok(m) => m,
                Err(e) => {
                    tracing::error!("cannot stat {path:?}: {e}");
                    continue;
                }
            };

            if meta.is_dir() {
                visit_dir(&path, root, settings, tx, progress_tx).await;
            } else if extension_matches(&path, &settings.extensions) {
                let Some(tags) = tags_for_path(&path, settings) else {
                    tracing::debug!("skipping ignored path: {path:?}");
                    continue;
                };
                if settings.dry_run {
                    tracing::info!("[dry_run] would scan: {path:?}");
                    let _ = progress_tx.send(ScanProgress::FileDiscovered).await;
                    continue;
                }
                if tx.send(TraversalItem { path, tags }).await.is_err() {
                    // Receiver dropped — pipeline shutting down.
                    return;
                }
                let _ = progress_tx.send(ScanProgress::FileDiscovered).await;
            }
        }
    })
}

// ---------------------------------------------------------------------------
// Stage 2 placeholder (implemented in Phase 3b)
// ---------------------------------------------------------------------------

async fn stage2_fingerprint(
    mut rx: mpsc::Receiver<TraversalItem>,
    tx: mpsc::Sender<ScannedFile>,
    concurrency: usize,
    _progress_tx: mpsc::Sender<ScanProgress>,
) {
    let mut join_set: tokio::task::JoinSet<()> = tokio::task::JoinSet::new();

    loop {
        // When at capacity, drain one completed task before receiving more.
        while join_set.len() >= concurrency {
            join_set.join_next().await;
        }

        match rx.recv().await {
            Some(item) => {
                let tx = tx.clone();
                join_set.spawn(async move {
                    match fingerprint_file(item).await {
                        Ok(scanned) => {
                            let _ = tx.send(scanned).await;
                        }
                        Err((path, e)) => {
                            tracing::error!("fingerprint error for {path:?}: {e}");
                        }
                    }
                });
            }
            None => break,
        }
    }

    // Drain any remaining in-flight tasks.
    while join_set.join_next().await.is_some() {}
}

async fn fingerprint_file(
    item: TraversalItem,
) -> Result<ScannedFile, (PathBuf, std::io::Error)> {
    use sha2::Digest as _;
    use tokio::io::AsyncReadExt as _;

    let extension = item
        .path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let file = tokio::fs::File::open(&item.path)
        .await
        .map_err(|e| (item.path.clone(), e))?;

    let meta = file
        .metadata()
        .await
        .map_err(|e| (item.path.clone(), e))?;

    let size = meta.len() as i64;

    let mut reader = tokio::io::BufReader::new(file);
    let mut hasher = sha2::Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader
            .read(&mut buf)
            .await
            .map_err(|e| (item.path.clone(), e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let fingerprint = format!("{:x}", hasher.finalize());

    Ok(ScannedFile {
        path: item.path,
        extension,
        size,
        fingerprint,
        tags: item.tags,
    })
}

// ---------------------------------------------------------------------------
// Stage 3 placeholder (implemented in Phase 3c)
// ---------------------------------------------------------------------------

async fn stage3_writer(
    mut rx: mpsc::Receiver<ScannedFile>,
    _pool: sqlx::SqlitePool,
    progress_tx: mpsc::Sender<ScanProgress>,
) {
    let mut discovered: u64 = 0;
    let mut processed: u64 = 0;
    let errors: u64 = 0;

    while let Some(file) = rx.recv().await {
        tracing::debug!("writer received: {:?}", file.path);
        // TODO: batch DB writes (Phase 3c)
        discovered += 1;
        processed += 1;
        let _ = progress_tx
            .send(ScanProgress::FileProcessed {
                path: file.path,
                was_new: true,
                was_updated: false,
            })
            .await;
    }

    let _ = progress_tx
        .send(ScanProgress::Completed {
            discovered,
            processed,
            errors,
        })
        .await;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;
    use crate::scan::ScanSettings;

    fn default_settings() -> ScanSettings {
        ScanSettings::default()
    }

    fn settings_with_ignore(dir: &Path) -> ScanSettings {
        let mut dirs = std::collections::BTreeMap::new();
        dirs.insert(
            dir.to_str().unwrap().parse().unwrap(),
            crate::scan::DirectorySettings::Ignore { inherit: false },
        );
        ScanSettings {
            directories: dirs,
            ..ScanSettings::default()
        }
    }

    fn settings_with_tags(dir: &Path, tags: Vec<String>) -> ScanSettings {
        let mut dirs = std::collections::BTreeMap::new();
        dirs.insert(
            dir.to_str().unwrap().parse().unwrap(),
            crate::scan::DirectorySettings::Scan {
                tags,
                inherit: false,
            },
        );
        ScanSettings {
            directories: dirs,
            ..ScanSettings::default()
        }
    }

    fn make_file(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, b"content").unwrap();
        path
    }

    fn make_dir(parent: &Path, name: &str) -> PathBuf {
        let path = parent.join(name);
        fs::create_dir_all(&path).unwrap();
        path
    }

    // Collect all TraversalItems sent over ch1 by running stage1 to completion.
    async fn collect_traversal(root: PathBuf, settings: ScanSettings) -> Vec<TraversalItem> {
        let (tx, mut rx) = mpsc::channel(64);
        let (progress_tx, _progress_rx) = mpsc::channel(64);
        stage1_traversal(root, &settings, tx, progress_tx).await;
        let mut items = Vec::new();
        while let Ok(item) = rx.try_recv() {
            items.push(item);
        }
        items
    }

    #[tokio::test]
    async fn traversal_finds_matching_extensions() {
        let tmp = TempDir::new().unwrap();
        make_file(tmp.path(), "book.pdf");
        make_file(tmp.path(), "book.epub");
        make_file(tmp.path(), "book.mobi");
        make_file(tmp.path(), "readme.txt");

        let items = collect_traversal(tmp.path().to_path_buf(), default_settings()).await;
        let mut paths: Vec<String> = items
            .iter()
            .map(|i| i.path.file_name().unwrap().to_str().unwrap().to_owned())
            .collect();
        paths.sort();

        assert_eq!(paths, vec!["book.epub", "book.mobi", "book.pdf"]);
    }

    #[tokio::test]
    async fn traversal_skips_hidden_files() {
        let tmp = TempDir::new().unwrap();
        make_file(tmp.path(), ".hidden.pdf");
        make_file(tmp.path(), "visible.pdf");

        let items = collect_traversal(tmp.path().to_path_buf(), default_settings()).await;
        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0].path.file_name().unwrap().to_str().unwrap(),
            "visible.pdf"
        );
    }

    #[tokio::test]
    async fn traversal_skips_hidden_directories() {
        let tmp = TempDir::new().unwrap();
        let hidden = make_dir(tmp.path(), ".hidden");
        make_file(&hidden, "book.pdf");
        make_file(tmp.path(), "visible.pdf");

        let items = collect_traversal(tmp.path().to_path_buf(), default_settings()).await;
        assert_eq!(items.len(), 1);
    }

    #[tokio::test]
    async fn traversal_stops_at_git_root() {
        let tmp = TempDir::new().unwrap();
        let repo = make_dir(tmp.path(), "my-project");
        make_dir(&repo, ".git");
        make_file(&repo, "paper.pdf");
        make_file(tmp.path(), "outside.pdf");

        let items = collect_traversal(tmp.path().to_path_buf(), default_settings()).await;
        let names: Vec<&str> = items
            .iter()
            .map(|i| i.path.file_name().unwrap().to_str().unwrap())
            .collect();
        assert!(!names.contains(&"paper.pdf"), "should not recurse into git repo");
        assert!(names.contains(&"outside.pdf"));
    }

    #[tokio::test]
    async fn traversal_stops_at_hg_root() {
        let tmp = TempDir::new().unwrap();
        let repo = make_dir(tmp.path(), "hg-project");
        make_dir(&repo, ".hg");
        make_file(&repo, "paper.pdf");

        let items = collect_traversal(tmp.path().to_path_buf(), default_settings()).await;
        assert!(items.is_empty());
    }

    #[tokio::test]
    async fn traversal_recurses_into_subdirs() {
        let tmp = TempDir::new().unwrap();
        let sub = make_dir(tmp.path(), "subdir");
        make_file(&sub, "book.pdf");

        let items = collect_traversal(tmp.path().to_path_buf(), default_settings()).await;
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].path, sub.join("book.pdf"));
    }

    #[tokio::test]
    async fn traversal_skips_ignored_directory() {
        let tmp = TempDir::new().unwrap();
        let ignored = make_dir(tmp.path(), "ignored");
        make_file(&ignored, "book.pdf");
        make_file(tmp.path(), "outside.pdf");

        let items = collect_traversal(
            tmp.path().to_path_buf(),
            settings_with_ignore(&ignored),
        )
        .await;
        let names: Vec<&str> = items
            .iter()
            .map(|i| i.path.file_name().unwrap().to_str().unwrap())
            .collect();
        assert!(!names.contains(&"book.pdf"));
        assert!(names.contains(&"outside.pdf"));
    }

    #[tokio::test]
    async fn traversal_attaches_tags_from_directory_settings() {
        let tmp = TempDir::new().unwrap();
        make_file(tmp.path(), "book.pdf");

        let items = collect_traversal(
            tmp.path().to_path_buf(),
            settings_with_tags(tmp.path(), vec!["fiction".into(), "2024".into()]),
        )
        .await;

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].tags, vec!["fiction", "2024"]);
    }

    #[tokio::test]
    async fn traversal_no_tags_when_no_directory_settings() {
        let tmp = TempDir::new().unwrap();
        make_file(tmp.path(), "book.pdf");

        let items = collect_traversal(tmp.path().to_path_buf(), default_settings()).await;
        assert_eq!(items.len(), 1);
        assert!(items[0].tags.is_empty());
    }

    #[tokio::test]
    async fn traversal_custom_extensions() {
        let tmp = TempDir::new().unwrap();
        make_file(tmp.path(), "comic.cbz");
        make_file(tmp.path(), "book.pdf");

        let settings = ScanSettings {
            extensions: vec!["cbz".into()],
            ..ScanSettings::default()
        };

        let items = collect_traversal(tmp.path().to_path_buf(), settings).await;
        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0].path.file_name().unwrap().to_str().unwrap(),
            "comic.cbz"
        );
    }

    #[tokio::test]
    async fn traversal_dry_run_emits_discovered_but_no_ch1_items() {
        let tmp = TempDir::new().unwrap();
        make_file(tmp.path(), "book.pdf");

        let settings = ScanSettings {
            dry_run: true,
            ..ScanSettings::default()
        };

        let (tx, mut rx) = mpsc::channel(64);
        let (progress_tx, mut progress_rx) = mpsc::channel(64);
        stage1_traversal(tmp.path().to_path_buf(), &settings, tx, progress_tx).await;

        // ch1 must be empty (no actual items sent in dry-run mode)
        assert!(rx.try_recv().is_err());
        // but a FileDiscovered progress event must have been emitted
        let ev = progress_rx.try_recv().unwrap();
        assert_eq!(ev, ScanProgress::FileDiscovered);
    }

    #[tokio::test]
    async fn fingerprint_file_produces_correct_sha256() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("test.pdf");
        fs::write(&path, b"hello world").unwrap();

        let item = TraversalItem {
            path: path.clone(),
            tags: vec![],
        };
        let scanned = fingerprint_file(item).await.unwrap();

        assert_eq!(scanned.extension, "pdf");
        assert_eq!(scanned.size, 11); // "hello world" is 11 bytes
        assert_eq!(scanned.fingerprint.len(), 64); // SHA-256 hex is 64 chars
        assert!(scanned.fingerprint.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(scanned.path, path);
    }

    #[tokio::test]
    async fn fingerprint_file_extension_lowercased() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("BOOK.PDF");
        fs::write(&path, b"data").unwrap();

        let item = TraversalItem {
            path,
            tags: vec![],
        };
        let scanned = fingerprint_file(item).await.unwrap();
        assert_eq!(scanned.extension, "pdf");
    }
}
