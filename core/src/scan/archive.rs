// SPDX-License-Identifier: AGPL-3.0-or-later

//! Reading document files stored inside archives (zipfiles and tarballs).
//!
//! Archive members are addressed by the pair `(archive_path, inner_path)`;
//! the scanner stores them in `files` with the synthetic unique path
//! `"{archive_path}::{inner_path}"`.

use std::collections::HashMap;
use std::collections::HashSet;
use std::io;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

/// Separator between the archive path and the member path in the synthetic
/// `files.path` value.
pub const ARCHIVE_PATH_SEPARATOR: &str = "::";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchiveKind {
    Zip,
    Tar,
    TarGz,
    TarBz2,
    TarXz,
    TarZstd,
}

fn archive_kind(path: &Path) -> Option<ArchiveKind> {
    let name = path.file_name()?.to_str()?.to_ascii_lowercase();
    use ArchiveKind::*;
    [
        (".tar.gz", TarGz),
        (".tgz", TarGz),
        (".tar.bz2", TarBz2),
        (".tbz2", TarBz2),
        (".tar.xz", TarXz),
        (".txz", TarXz),
        (".tar.zst", TarZstd),
        (".tar.zstd", TarZstd),
        (".tzst", TarZstd),
        (".tar", Tar),
        (".zip", Zip),
    ]
    .into_iter()
    .find(|(suffix, _)| name.ends_with(suffix))
    .map(|(_, kind)| kind)
}

/// Whether `path` looks like a supported archive (zip, tar, tar.gz/tgz,
/// tar.bz2/tbz2, tar.xz/txz, tar.zst/tar.zstd/tzst).
pub fn is_archive_path(path: &Path) -> bool {
    archive_kind(path).is_some()
}

/// Whether `path` is a tar-based archive. Unlike zip, tar has no random
/// access: reading any member requires decompressing the stream from the
/// start, which is what single-pass spooling avoids repeating.
pub fn is_tar_archive_path(path: &Path) -> bool {
    matches!(archive_kind(path), Some(kind) if kind != ArchiveKind::Zip)
}

/// Build the synthetic unique `files.path` value for an archive member.
pub fn joined_archive_path(archive: &Path, inner: &str) -> String {
    format!("{}{ARCHIVE_PATH_SEPARATOR}{inner}", archive.display())
}

fn tar_reader(path: &Path, kind: ArchiveKind) -> io::Result<Box<dyn Read>> {
    let file = std::fs::File::open(path)?;
    Ok(match kind {
        ArchiveKind::Tar => Box::new(file),
        ArchiveKind::TarGz => Box::new(flate2::read::GzDecoder::new(file)),
        ArchiveKind::TarBz2 => Box::new(bzip2::read::BzDecoder::new(file)),
        ArchiveKind::TarXz => Box::new(xz2::read::XzDecoder::new(file)),
        ArchiveKind::TarZstd => Box::new(zstd::stream::read::Decoder::new(file)?),
        ArchiveKind::Zip => unreachable!("zip handled separately"),
    })
}

fn not_an_archive(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("not a supported archive: {path:?}"),
    )
}

fn member_not_found(archive: &Path, inner: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("member {inner:?} not found in archive {archive:?}"),
    )
}

/// List the inner paths of all regular-file members of the archive.
/// Filtering by document extension is the caller's responsibility.
///
/// Blocking I/O — call from a blocking context (e.g. `spawn_blocking`).
pub fn enumerate_archive_members(path: &Path) -> io::Result<Vec<String>> {
    match archive_kind(path).ok_or_else(|| not_an_archive(path))? {
        ArchiveKind::Zip => {
            let file = std::fs::File::open(path)?;
            let archive = zip::ZipArchive::new(file).map_err(io::Error::other)?;
            Ok(archive
                .file_names()
                .filter(|name| !name.ends_with('/'))
                .map(str::to_owned)
                .collect())
        }
        kind => {
            let mut archive = tar::Archive::new(tar_reader(path, kind)?);
            archive
                .entries()?
                .filter_map(|entry| {
                    let entry = match entry {
                        Ok(e) => e,
                        Err(e) => return Some(Err(e)),
                    };
                    entry
                        .header()
                        .entry_type()
                        .is_file()
                        .then(|| entry.path().map(|p| p.to_string_lossy().into_owned()))
                })
                .collect()
        }
    }
}

/// An archive member extracted to a spool directory shared by all members of
/// one archive. Clones share the directory guard; the directory (and every
/// spooled file in it) is deleted when the last clone drops.
#[derive(Debug, Clone)]
pub struct SpooledFile {
    pub path: PathBuf,
    _dir: Arc<tempfile::TempDir>,
}

/// Extract the `wanted` members of a tar-based archive in a single
/// decompression pass, spooling them into one temp directory. Returns
/// inner path → spooled file; members missing from the archive are simply
/// absent from the map. Spooled files keep the member's extension so
/// format-sniffing readers (MuPDF et al.) can open them.
///
/// Blocking I/O — call from a blocking context (e.g. `spawn_blocking`).
pub fn spool_tar_archive(
    archive_path: &Path,
    wanted: &HashSet<String>,
) -> io::Result<HashMap<String, SpooledFile>> {
    let kind = archive_kind(archive_path).ok_or_else(|| not_an_archive(archive_path))?;
    if kind == ArchiveKind::Zip {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "zip archives have random access; single-pass spooling is tar-only",
        ));
    }

    let dir = Arc::new(tempfile::TempDir::with_prefix("read-flow-spool-")?);
    let mut spooled: HashMap<String, SpooledFile> = HashMap::new();

    let mut archive = tar::Archive::new(tar_reader(archive_path, kind)?);
    for (index, entry) in archive.entries()?.enumerate() {
        if spooled.len() == wanted.len() {
            break; // all wanted members found — stop decompressing early
        }
        let mut entry = entry?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let inner = entry.path()?.to_string_lossy().into_owned();
        if !wanted.contains(&inner) {
            continue;
        }
        // Index-based names avoid collisions and path traversal from member names.
        let extension = Path::new(&inner)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("bin");
        let target = dir.path().join(format!("{index}.{extension}"));
        let mut file = std::fs::File::create(&target)?;
        io::copy(&mut entry, &mut file)?;
        spooled.insert(
            inner,
            SpooledFile {
                path: target,
                _dir: Arc::clone(&dir),
            },
        );
    }

    Ok(spooled)
}

/// Extract a single member (by inner path) from the archive and return its bytes.
///
/// Blocking I/O — call from a blocking context (e.g. `spawn_blocking`).
// TODO nested-archives: to support archives inside archives, recurse here on
// `::`-separated inner segments, extracting each level to memory/temp first.
pub fn extract_archive_member(archive_path: &Path, inner: &str) -> io::Result<Vec<u8>> {
    match archive_kind(archive_path).ok_or_else(|| not_an_archive(archive_path))? {
        ArchiveKind::Zip => {
            let file = std::fs::File::open(archive_path)?;
            let mut archive = zip::ZipArchive::new(file).map_err(io::Error::other)?;
            let mut member = archive.by_name(inner).map_err(|e| match e {
                zip::result::ZipError::FileNotFound => member_not_found(archive_path, inner),
                other => io::Error::other(other),
            })?;
            let mut bytes = Vec::with_capacity(member.size() as usize);
            member.read_to_end(&mut bytes)?;
            Ok(bytes)
        }
        kind => {
            let mut archive = tar::Archive::new(tar_reader(archive_path, kind)?);
            for entry in archive.entries()? {
                let mut entry = entry?;
                if entry.header().entry_type().is_file() && entry.path()?.as_os_str() == inner {
                    let mut bytes = Vec::with_capacity(entry.size() as usize);
                    entry.read_to_end(&mut bytes)?;
                    return Ok(bytes);
                }
            }
            Err(member_not_found(archive_path, inner))
        }
    }
}

/// Extract an archive member to a stable cached location
/// (`{temp}/read-flow/{key}.{extension}`) so local readers can open it like a
/// regular file. Repeat calls with the same `key` reuse the extracted copy.
///
/// Blocking I/O — call from a blocking context (e.g. `spawn_blocking`).
pub fn extract_member_to_cache(
    archive_path: &Path,
    inner: &str,
    key: &str,
    extension: &str,
) -> io::Result<std::path::PathBuf> {
    let dir = std::env::temp_dir().join("read-flow");
    std::fs::create_dir_all(&dir)?;
    let target = dir.join(format!("{key}.{extension}"));
    if !target.exists() {
        let bytes = extract_archive_member(archive_path, inner)?;
        std::fs::write(&target, bytes)?;
    }
    Ok(target)
}

#[cfg(test)]
mod tests {
    use std::io::Write as _;
    use std::path::PathBuf;

    use assert4rs::Assert;
    use rstest::rstest;
    use tempfile::TempDir;

    use super::*;

    fn make_zip(dir: &Path, name: &str, members: &[(&str, &[u8])]) -> PathBuf {
        let path = dir.join(name);
        let file = std::fs::File::create(&path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        for (member, data) in members {
            writer.start_file(*member, options).unwrap();
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap();
        path
    }

    fn make_tar_gz(dir: &Path, name: &str, members: &[(&str, &[u8])]) -> PathBuf {
        let path = dir.join(name);
        let file = std::fs::File::create(&path).unwrap();
        let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut builder = tar::Builder::new(encoder);
        for (member, data) in members {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, member, *data).unwrap();
        }
        builder.into_inner().unwrap().finish().unwrap();
        path
    }

    fn make_tar_zst(dir: &Path, name: &str, members: &[(&str, &[u8])]) -> PathBuf {
        let path = dir.join(name);
        let file = std::fs::File::create(&path).unwrap();
        let encoder = zstd::stream::write::Encoder::new(file, 0)
            .unwrap()
            .auto_finish();
        let mut builder = tar::Builder::new(encoder);
        for (member, data) in members {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, member, *data).unwrap();
        }
        builder.into_inner().unwrap();
        path
    }

    #[rstest]
    #[case("library.zip", true)]
    #[case("library.ZIP", true)]
    #[case("backup.tar", true)]
    #[case("backup.tar.gz", true)]
    #[case("backup.tgz", true)]
    #[case("backup.tar.bz2", true)]
    #[case("backup.tbz2", true)]
    #[case("backup.tar.xz", true)]
    #[case("backup.txz", true)]
    #[case("backup.tar.zst", true)]
    #[case("backup.tar.zstd", true)]
    #[case("backup.tzst", true)]
    #[case("book.pdf", false)]
    #[case("comic.cbz", false)]
    #[case("archive.rar", false)]
    #[case("noextension", false)]
    fn detects_archive_paths(#[case] name: &str, #[case] expected: bool) {
        Assert::that(is_archive_path(Path::new(name))).is(expected);
    }

    #[test]
    fn joined_archive_path_uses_separator() {
        let joined = joined_archive_path(Path::new("/data/lib.zip"), "books/novel.epub");
        Assert::that(joined).is("/data/lib.zip::books/novel.epub");
    }

    #[test]
    fn enumerates_zip_members() {
        let tmp = TempDir::new().unwrap();
        let zip = make_zip(
            tmp.path(),
            "lib.zip",
            &[("books/a.epub", b"epub-a"), ("b.pdf", b"pdf-b")],
        );

        let mut members = enumerate_archive_members(&zip).unwrap();
        members.sort();
        Assert::that(members).is(vec!["b.pdf", "books/a.epub"]);
    }

    #[test]
    fn enumerates_tar_gz_members() {
        let tmp = TempDir::new().unwrap();
        let tarball = make_tar_gz(
            tmp.path(),
            "lib.tar.gz",
            &[("books/a.epub", b"epub-a"), ("b.pdf", b"pdf-b")],
        );

        let mut members = enumerate_archive_members(&tarball).unwrap();
        members.sort();
        Assert::that(members).is(vec!["b.pdf", "books/a.epub"]);
    }

    #[test]
    fn enumerates_tar_zst_members() {
        let tmp = TempDir::new().unwrap();
        let tarball = make_tar_zst(
            tmp.path(),
            "lib.tar.zst",
            &[("books/a.epub", b"epub-a"), ("b.pdf", b"pdf-b")],
        );

        let mut members = enumerate_archive_members(&tarball).unwrap();
        members.sort();
        Assert::that(members).is(vec!["b.pdf", "books/a.epub"]);
    }

    #[test]
    fn extracts_zip_member_bytes() {
        let tmp = TempDir::new().unwrap();
        let zip = make_zip(tmp.path(), "lib.zip", &[("books/a.epub", b"epub-bytes")]);

        let bytes = extract_archive_member(&zip, "books/a.epub").unwrap();
        Assert::that(bytes).is(b"epub-bytes");
    }

    #[test]
    fn extracts_tar_gz_member_bytes() {
        let tmp = TempDir::new().unwrap();
        let tarball = make_tar_gz(tmp.path(), "lib.tar.gz", &[("b.pdf", b"pdf-bytes")]);

        let bytes = extract_archive_member(&tarball, "b.pdf").unwrap();
        Assert::that(bytes).is(b"pdf-bytes");
    }

    #[test]
    fn extracts_tar_zst_member_bytes() {
        let tmp = TempDir::new().unwrap();
        let tarball = make_tar_zst(tmp.path(), "lib.tar.zst", &[("b.pdf", b"pdf-bytes")]);

        let bytes = extract_archive_member(&tarball, "b.pdf").unwrap();
        Assert::that(bytes).is(b"pdf-bytes");
    }

    #[test]
    fn extract_missing_member_is_not_found() {
        let tmp = TempDir::new().unwrap();
        let zip = make_zip(tmp.path(), "lib.zip", &[("a.epub", b"x")]);

        let err = extract_archive_member(&zip, "missing.pdf").unwrap_err();
        Assert::that(err.kind()).is(io::ErrorKind::NotFound);
    }

    #[test]
    fn extract_member_to_cache_creates_and_reuses_file() {
        let tmp = TempDir::new().unwrap();
        let zip = make_zip(tmp.path(), "lib.zip", &[("a.epub", b"first")]);
        let key = format!("test-cache-{}", std::process::id());

        let target = extract_member_to_cache(&zip, "a.epub", &key, "epub").unwrap();
        Assert::that(std::fs::read(&target).unwrap()).is(b"first");

        // Second call reuses the cached copy even if the archive changed.
        let zip = make_zip(tmp.path(), "lib.zip", &[("a.epub", b"second")]);
        let again = extract_member_to_cache(&zip, "a.epub", &key, "epub").unwrap();
        Assert::that(again.clone()).is(target.clone());
        Assert::that(std::fs::read(&again).unwrap()).is(b"first");

        std::fs::remove_file(target).unwrap();
    }

    #[rstest]
    #[case("backup.tar", true)]
    #[case("backup.tar.gz", true)]
    #[case("backup.txz", true)]
    #[case("backup.tar.zst", true)]
    #[case("library.zip", false)]
    #[case("book.pdf", false)]
    fn detects_tar_archive_paths(#[case] name: &str, #[case] expected: bool) {
        Assert::that(is_tar_archive_path(Path::new(name))).is(expected);
    }

    #[test]
    fn spool_tar_extracts_wanted_members_in_one_pass() {
        let tmp = TempDir::new().unwrap();
        let tarball = make_tar_gz(
            tmp.path(),
            "lib.tar.gz",
            &[
                ("books/a.epub", b"epub-a"),
                ("notes.txt", b"skip me"),
                ("b.pdf", b"pdf-b"),
            ],
        );

        let wanted: HashSet<String> = ["books/a.epub".to_string(), "b.pdf".to_string()].into();
        let spooled = spool_tar_archive(&tarball, &wanted).unwrap();

        Assert::that(&spooled).has_length(2);
        Assert::that(std::fs::read(&spooled["books/a.epub"].path).unwrap()).is(b"epub-a");
        Assert::that(std::fs::read(&spooled["b.pdf"].path).unwrap()).is(b"pdf-b");
        // Spooled files keep the member extension for format sniffing.
        Assert::that(spooled["books/a.epub"].path.extension().unwrap()).is("epub");
    }

    #[test]
    fn spool_tar_missing_members_are_absent_from_map() {
        let tmp = TempDir::new().unwrap();
        let tarball = make_tar_gz(tmp.path(), "lib.tar.gz", &[("a.epub", b"x")]);

        let wanted: HashSet<String> = ["a.epub".to_string(), "missing.pdf".to_string()].into();
        let spooled = spool_tar_archive(&tarball, &wanted).unwrap();
        Assert::that(&spooled).has_length(1);
        assert!(spooled.contains_key("a.epub"));
    }

    #[test]
    fn spool_dir_deleted_when_last_clone_drops() {
        let tmp = TempDir::new().unwrap();
        let tarball = make_tar_gz(tmp.path(), "lib.tar.gz", &[("a.epub", b"x")]);

        let wanted: HashSet<String> = ["a.epub".to_string()].into();
        let spooled = spool_tar_archive(&tarball, &wanted).unwrap();
        let file = spooled["a.epub"].clone();
        let path = file.path.clone();

        drop(spooled);
        assert!(path.exists(), "clone must keep the spool dir alive");
        drop(file);
        assert!(!path.exists(), "dropping the last clone deletes the spool");
    }

    #[test]
    fn spool_rejects_zip() {
        let tmp = TempDir::new().unwrap();
        let zip = make_zip(tmp.path(), "lib.zip", &[("a.epub", b"x")]);
        let err = spool_tar_archive(&zip, &HashSet::new()).unwrap_err();
        Assert::that(err.kind()).is(io::ErrorKind::InvalidInput);
    }

    #[test]
    fn enumerate_non_archive_is_invalid_input() {
        let err = enumerate_archive_members(Path::new("/tmp/book.pdf")).unwrap_err();
        Assert::that(err.kind()).is(io::ErrorKind::InvalidInput);
    }
}
