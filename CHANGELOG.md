# Changelog

All notable changes to Read Flow are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

The version below refers to the **product** (the `read-flow` application). Individual
workspace crates may carry their own versions; see [RELEASING.md](RELEASING.md).

## [Unreleased]

<!-- Add entries here as you land changes. Move them under a version heading at release time. -->

### Added

- Archive scanning supports zstd-compressed tarballs (`.tar.zst`, `.tar.zstd`, `.tzst`).

### Changed

### Fixed

## [0.1.0] - 2026-07-06

First public release.

### Added

- **Document scanning** with content-based (SHA-256) fingerprinting and duplicate detection.
- **Tags and auto-tagging**, including rules that tag documents automatically as they are found.
- **Reading status and progress tracking** (Unread / Reading / Read), synced across devices.
- **Built-in readers**: PDF viewer (derived from pop-os/cosmic-reader) and a native EPUB reader.
- **Online libraries (OPDS)**: search catalogs such as Project Gutenberg and Standard Ebooks.
- **Fuzzy search** in the web app (PWA).
- **Private mode** to hide sensitive documents behind a private-tag filter.
- **Interfaces**: COSMIC desktop app (Linux + macOS), headless server (`read-flow --headless`),
  and a SvelteKit Progressive Web App.
- **Internationalization** of the desktop app in English, French, and Dutch.
- Supported formats: PDF, EPUB, MOBI, FB2, CBZ/CBT, DOCX/XLSX/PPTX/XPS, and documents in archives.

<!-- Link references. Update the compare URLs when the repo is on GitHub. -->
[Unreleased]: https://github.com/read-flow/read-flow/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/read-flow/read-flow/releases/tag/v0.1.0
