# Changelog

All notable changes to Read Flow are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

The version below refers to the **product** (the `read-flow` application). Individual
workspace crates may carry their own versions; see [RELEASING.md](RELEASING.md).

## [Unreleased]

<!-- Add entries here as you land changes. Move them under a version heading at release time. -->

### Added

- COSMIC Document Details: each source now shows when it was first imported into the library
  (e.g. "Added Jul 15, 2026"), next to the source's file path.
- COSMIC: after a library scan finishes, clicking the "Last scan: …" footer opens a report
  dialog showing how many files were added/updated, plus a list of any files that failed with
  their error message. The REST `POST /scan` response also carries these new `added`/`updated`/
  `error_details` fields alongside the existing counts.
- COSMIC and PWA document list: new "Date Added" sort option, so recently-imported documents
  (including a document that just had a new format added to an existing one) can be found
  quickly.
- COSMIC PDF viewer: context pane now offers an "Open in another viewer" → "External viewer
  (system default)" action for the current document, matching the EPUB viewer's existing
  open-in-external option.

### Changed

### Fixed

## [0.2.0] - 2026-07-18

### Added

- Per-app theme overrides in Preferences → Appearance: accent color, density, roundness,
  frosted glass (Linux/COSMIC only), interface font, and advanced background colors — without
  changing the global COSMIC settings (`[ui.theme]` in read-flow.toml). Light and dark are
  configured independently and both saved at once, so the app switches between them live to
  match the system's current dark/light mode instead of being pinned to one.
- COSMIC Preferences → Online Library: add/edit/remove custom OPDS catalogs, and enable/disable
  the built-in ones (Project Gutenberg, Standard Ebooks). Previously the only way to change
  catalog configuration was hand-editing `read-flow.toml`.

### Changed

- COSMIC Preferences → Appearance: added row icons to theme settings (accent, background,
  density, roundness, frosted glass, font, font size) for easier scanning.
- PWA document details: the cover thumbnail now stays visible while editing metadata, sitting
  beside the edit form on wide screens instead of disappearing.
- COSMIC document details: the cover thumbnail now stays visible (and grows) while editing
  metadata, and edit fields stack label-above-input instead of splitting the row in half.
- COSMIC: removed the global "EPUB viewer preference" setting from Preferences → Appearance.
  The EPUB viewer's context pane now offers "Open in MuPDF viewer" and "Open in external
  viewer" actions for the current document instead, since the native viewer handles most
  EPUBs well enough that a global switch is no longer necessary. Reading progress for each
  viewer is now stored side by side per document, so switching between them resumes each one
  from its own last position instead of one overwriting the other's.
- **Breaking:** Online library: built-in catalogs (Project Gutenberg, Standard Ebooks) are no
  longer stored by name/URL in `read-flow.toml` — only their id and enabled state are, so their
  search URLs are always the current code default and can never go stale. The
  `[[online_library.catalogs]]` table shape changed to a tagged format and **there is no
  automatic migration** — a `read-flow.toml` from before this change will fail to load. If you
  have an existing `read-flow.toml`, remove its `online_library.catalogs` entries (or the whole
  `[online_library]` section) before upgrading; the app recreates the default built-in catalogs
  (both enabled) on next start. Any catalog you'd added yourself needs to be re-added by hand in
  the new format:
  ```toml
  [[online_library.catalogs]]
  type = "builtin"
  id = "project_gutenberg" # or "standard_ebooks"
  enabled = true

  [[online_library.catalogs]]
  type = "configured"
  name = "My Library"
  search_url = "https://example.com/opds?q={searchTerms}"
  enabled = true
  ```

### Fixed

- COSMIC online library: the catalog filter list in the context pane was empty until the first
  search completed, since the page only populated its catalog list as a side effect of a search
  response. The page now loads and resolves configured catalogs on its own when opened, and
  reloads them whenever settings change, so the filter list is populated immediately and stays
  in sync without needing another search.

## [0.1.1] - 2026-07-12

### Added

- Archive scanning supports zstd-compressed tarballs (`.tar.zst`, `.tar.zstd`, `.tzst`).
- Linux arm64 release builds (native `.deb` + portable tarball, e.g. for Raspberry Pi 5),
  alongside the existing x86_64 build.
- Flatpak packaging (`flatpak/io.github.read-flow.yml`), built to a `.flatpak` bundle and
  attached to releases by CI. First step toward a Flathub submission — see RELEASING.md.

### Changed

- Relicensed `read-flow-core`, `read-flow`, and `read-flow-widgets` as AGPL-3.0-or-later (was
  MIT / GPL-3.0-or-later); see `NOTICE` for why.
- Application ID renamed `com.github.read-flow.read-flow` → `io.github.read-flow` (two steps: first
  to `io.github.read-flow.read-flow` for the `io.github.<owner>.<repo>` convention Flathub requires
  for GitHub-hosted apps, then collapsed to the 3-segment `io.github.read-flow` because Flatpak app
  IDs only permit a hyphen in the *last* segment — `read-flow` the org and `read-flow` the repo both
  have one, so the 4-segment form was invalid; dropping the redundant repo segment, since org and
  repo share a name here, sidesteps that). **Existing local installs will see their desktop-app
  preferences (theme, window state) reset once**, since `cosmic-config` stores them under a path
  keyed by the app ID — reading progress, tags, and the document library (SQLite) are unaffected,
  only COSMIC UI prefs.

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
[Unreleased]: https://github.com/read-flow/read-flow/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/read-flow/read-flow/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/read-flow/read-flow/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/read-flow/read-flow/releases/tag/v0.1.0
