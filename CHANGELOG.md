# Changelog

All notable changes to Read Flow are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

The version below refers to the **product** (the `read-flow` application). Individual
workspace crates may carry their own versions; see [RELEASING.md](RELEASING.md).

## [Unreleased]

### Added

- New `--debug` flag. The EPUB viewer's "Show raw HTML" toggle (a developer-only debugging aid)
  is now hidden unless the app is launched with `--debug`.

### Changed

- Preferences → Server: CORS allowed origins, upload size limit, and HTTPS/TLS (certificates,
  self-signed generation) now live behind a collapsed "Advanced" toggle. All three already default
  to safe values (unrestricted CORS, 100 MiB uploads, no TLS) and only matter once the server is
  reachable past your own device. Bind address and port stay visible, since those are needed for
  the PWA to work at all.
- The language switcher moved from the top menu bar (View/Actions/Language) into Preferences →
  Appearance, and now defaults to "Follow System" instead of always starting in English — the app
  already read the OS locale at startup, but a manual menu pick was never remembered across
  restarts. An explicit choice is now saved and re-applied live without restarting.
- Preferences → Server → Local Identity now includes a worked example of what picking a user
  there actually does, instead of only abstract wording.

### Removed

- Cover extraction no longer trims whitespace borders from extracted/resized covers.

### Fixed

- Dashboard "Continue Reading" cover thumbnails no longer bleed outside their card borders when
  rendered by the software (tiny-skia) backend, as used by the marketing-screenshot tool.
- Closing the document details page after editing tags no longer rescans the entire document
  library to refresh the tag suggestion list, which was slow on large (8000+) libraries. The
  document list also no longer keeps a second, duplicate in-memory copy of the whole library for
  this, halving its memory footprint.
- Document details cover-selection thumbnails now wrap onto additional rows instead of being
  squeezed to fit when a document has 4 or more attached files.
- The document merge dialog now shows a cover thumbnail, authors, and format/tag pills for each
  candidate document, states the concrete consequence of the merge (which documents will be
  deleted and where their files move) once a winner is picked, and uses a destructive-styled
  confirm button, matching the confirmation pattern already used for purging missing files.
- The EPUB viewer's in-book search now says "No matches in this chapter" instead of a bare "No
  matches", since search only ever looked within the currently open chapter.

<!-- Add entries here as you land changes. Move them under a version heading at release time. -->

## [0.4.1] - 2026-08-04

### Changed

- Release pipeline: build Flatpak bundles for arm64 (aarch64) alongside x86_64. Bumped the
  Freedesktop SDK runtime to 25.08.

## [0.4.0] - 2026-08-03

### Added

- Server: `server.local_user_id` setting binds the desktop app's local database access to a
  designated authorized user, so reading progress and tags recorded on the desktop are shared with
  that user's remote (REST/PWA) sessions. Configurable from COSMIC Preferences → Server → Local
  Identity.
- COSMIC Preferences → Server → HTTPS: "Generate certificate" now issues a certificate signed by a
  local certificate authority (generated once, reused after) instead of a plain self-signed one.
  Trust that CA root once per client device — a new "Local CA certificate" action opens it with the
  OS's own certificate-import flow (e.g. Keychain Access on macOS) — and every certificate generated
  afterwards, including after regenerating one for a new bind address, is trusted automatically with
  no further per-device action or browser warnings. Devices can also fetch the CA root directly from
  a running server at `GET /ca.pem`.

### Changed

- COSMIC Preferences: adding/editing a scan directory, source, authorized user, or online-library
  catalog now opens a modal dialog instead of expanding an inline form below the list.
- COSMIC Preferences: removing a scan directory, authorized user, or online-library catalog now
  asks for confirmation first, matching the existing behavior for removing a source.
- Server: the `ui.private_mode` toggle no longer disables private-tag filtering for remote API
  requests; it now only controls the local GUI. Remote/PWA clients must request private content
  explicitly via the `x-private-mode` header (and must own that content).

### Fixed

- COSMIC Preferences: the Online Library section now shows the "Save Settings"/"Revert" row, so
  catalog changes made there have a visible way to be saved instead of only persisting if you
  happen to click Save from another section afterward.
- Server: fixed several endpoints (file update; reading-state get/put/status; document
  list/get/cover/metadata/merge/ensure) that skipped private-content filtering, letting any
  authenticated user read or mutate documents hidden by a private tag. All content endpoints now go
  through one shared visibility check.
- Server: Basic-auth password verification (Argon2/PBKDF2) now runs off the async worker threads
  instead of blocking them, and failed Basic-auth attempts are rate-limited (10 failures/60s) on
  every endpoint, not just `/oauth/token`, closing a brute-force bypass.
- Settings (`read-flow.toml`) writes from the REST admin API, COSMIC preferences, and the embedded
  server are now serialized and atomic, so concurrent saves can no longer silently overwrite each
  other's changes, and a crash mid-write can no longer truncate the config.
- Deleting a document whose file lives inside an archive (e.g. a `.zip`/`.tar` member) no longer
  fails, and deleting a document whose underlying file is already missing on disk now succeeds
  instead of being blocked.
- Fixed a crash on an invalid stored reading-status value (now degrades to Unread with a warning)
  and on certain file/document write paths reading back their own row (now surfaces as an error
  instead of panicking).
- PWA: reading progress saved by COSMIC's PDF or EPUB viewer no longer gets ignored when opening the
  same document in the PWA (it always restarted from the beginning). The PWA readers now understand
  COSMIC's combined per-viewer position format, and preserve it when saving their own progress.
- COSMIC: configuring `server.local_user_id` from Preferences → Server → Local Identity now takes
  effect immediately for local reading progress/tags, instead of only after restarting the
  app. Saving settings (from the GUI or the REST admin API) previously left the cached local
  database client bound to the old resolved user, so local writes kept landing under the previous
  identity until the process restarted.
- PDF reading progress saved by COSMIC and opened in the PWA (or vice versa) was off by one page,
  since COSMIC stored its 0-based page index directly while the PWA stores/reads a 1-based page
  number. Both now agree on a 1-based page number on the wire.
- COSMIC Dashboard: the "Continue Reading" row no longer stretches its cards to fill the whole
  width when fewer than 4 documents are in progress. It's padded with invisible placeholders up to
  4, so cards keep the same size regardless of how many are shown.
- COSMIC: reading progress for open EPUB/PDF viewers is now saved when the application window is
  closed, not just when each viewer tab is closed individually.

## [0.3.2] - 2026-07-23

### Changed

- Enable link time optimization (LTO) for release builds.

## [0.3.1] - 2026-07-23

### Fixed

- COSMIC EPUB viewer: make the left/right click-to-turn-page zones transparant instead of using
  the main content background. This aligns the look and feel with the MuPDF viewer and looks
  better with COSMIC's frosted glass effect (Linux).

## [0.3.0] - 2026-07-20

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
- COSMIC Preferences → Appearance: new "Monospace font" picker, applied live to code blocks and
  preformatted text (EPUB viewer, OPDS description rendering), alongside the existing interface
  font picker.

### Changed

- COSMIC EPUB viewer: the left/right click-to-turn-page zones now use the main content
  background instead of the surrounding "desk" background, so they blend with the page area —
  looks better with COSMIC's frosted glass effect (Linux).

### Fixed

### Removed

- COSMIC Preferences → Appearance: removed the "Interface font size" field. It never had any
  visible effect — COSMIC's own text widgets hardcode their point sizes and ignore the renderer
  default this setting controlled — so it was a dead control rather than a working one.

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
[Unreleased]: https://github.com/read-flow/read-flow/compare/v0.4.1...HEAD
[0.4.1]: https://github.com/read-flow/read-flow/compare/v0.4.0...v0.4.1
[0.4.0]: https://github.com/read-flow/read-flow/compare/v0.3.2...v0.4.0
[0.3.2]: https://github.com/read-flow/read-flow/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/read-flow/read-flow/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/read-flow/read-flow/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/read-flow/read-flow/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/read-flow/read-flow/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/read-flow/read-flow/releases/tag/v0.1.0
