# Scan report dialog + per-file `imported_at`

## Context

Two related but independent asks:

1. After a library scan, the user only sees a persistent footer ("Last scan: X discovered, Y
   processed, Z errors") with no way to see *which* files were added/updated, or what actually
   failed and why. The underlying per-file data (`was_new`/`was_updated` on success,
   `path`/`error` on failure) already flows through the scan pipeline's event stream
   (`ScanProgress::FileProcessed`/`FileError`, `core/src/scan/pipeline.rs`) but is discarded —
   `ScanSummary::add_event` (`core/src/scan/mod.rs:26-39`) only folds `Completed` events, and
   `ScanComponent::update` (`cosmic/src/component/scan_progress.rs:50-58`) matches
   `FileProcessed { .. }`/`FileError { .. }`, throwing the fields away.
2. Individual file rows in the `files` table (`core/migrations/20240925070820_initial_database.sql:18-29`)
   have no timestamp at all — no way to know when a given file was first imported into the
   library. This spec adds `imported_at` and surfaces it in Document Details' source list.

Both extend existing, already-BDD-covered features (`admin.scan`, `documents.detail_view` in
`FEATURES.toml`) rather than introducing new capability boundaries.

## Feature A — Scan report dialog

### Data layer

**`core/src/scan/mod.rs`** — extend `ScanSummary`:

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScanFileError {
    pub path: String,
    pub message: String,
}

pub struct ScanSummary {
    pub discovered: u64,
    pub processed: u64,
    pub errors: u64,
    pub added: u64,
    pub updated: u64,
    pub error_details: Vec<ScanFileError>,
}
```

`add_event` additionally matches `ScanProgress::FileProcessed { was_new, was_updated, .. }` (increment
`added` or `updated`) and `ScanProgress::FileError { path, error }` (increment `errors`, push a
`ScanFileError`) — alongside the existing `Completed` fold, which stays as-is for
`discovered`/`processed`. `ScanSummary` is already `Serialize`/`Deserialize` and is the literal
`POST /scan` response body (`core/src/server/mod.rs:1247`); these are additive fields, non-breaking
for existing REST clients.

### `cosmic/src/component/scan_progress.rs` (`ScanComponent`)

- New fields: `added: u64`, `updated: u64`, `error_details: Vec<(String, String)>` (path, message),
  `report_open: bool`.
- `ScanProgressMessage` gains `ViewReport` and `CloseReport` (purely internal to the component —
  no new `ScanProgressOutput` variant needed, `update()` just flips `report_open` and returns
  `Task::none()`).
- `update()`'s `Progress(event)` arm: `FileProcessed { was_new, was_updated, .. }` → increment
  `added` (if `was_new`) or `updated` (if `was_updated`, not new); `FileError { path, error }` →
  push `(path.display().to_string(), error)` in addition to the existing `errors += 1`.
- `view()`: once `!self.active` (scan finished), wrap the existing "Last scan: …" label in
  `.apply(widget::mouse_area).on_press(ScanProgressMessage::ViewReport)` — same clickable-row
  pattern Preferences' Overview cards already use
  (`cosmic/src/page/preferences.rs:560-568`). While `self.active` (scan in progress), the label
  stays non-interactive as today.
- New `pub fn dialog(&self) -> Option<Element<'_, ScanProgressMessage>>`, gated on
  `self.report_open`, following `CheckMissingComponent::dialog()`
  (`cosmic/src/component/check_missing.rs:82`) exactly: `widget::dialog()` with a title, a summary
  body line (`"{added} added, {updated} updated, {errors} errors"`), and — only when
  `error_details` is non-empty — a scrollable `widget::column` of `path: message` rows (same
  scrollable-list widget check_missing's dialog already uses for its file list), plus a "Close"
  primary action wired to `CloseReport`.

### `cosmic/src/app.rs`

- `fn dialog()` (line 496): add a branch for `self.scan_component`, mirroring the existing
  `check_missing_component` branch — `component.dialog().map(|e| e.map(Message::ScanComponent))`.

### i18n (en/fr/nl)

New keys: `scan-report-title`, `scan-report-summary` (with `$added`/`$updated`/`$errors`
placeables), `scan-report-errors-section`, `scan-report-close`.

### Testing

- `ScanSummary::add_event` gets inline unit tests covering: a `FileProcessed{was_new: true}` event
  increments `added`; `was_new: false, was_updated: true` increments `updated`; a `FileError`
  increments `errors` and appends to `error_details`; a `Completed` event still folds
  `discovered`/`processed` as before (regression coverage for existing behavior).
- `features/admin_scan.feature` gets a new scenario asserting the report's added/updated/error
  counts after a scan (exact wording to match the existing scenario's Given/When style), with a
  new step in `cosmic/src/bdd/steps/admin_scan.rs` calling into whichever driver-level accessor
  exposes the richer `ScanSummary` (REST: parse the `POST /scan` JSON body directly, already
  returns the new fields for free; COSMIC: needs a driver method — likely calling
  `application_module.scan_configured()` directly and inspecting the returned `ScanSummary`, same
  bypass-the-UI approach used by `admin_authorized_users`/`admin_scan_directories` steps).

## Feature B — `imported_at` per file

### Migration

New file `core/migrations/<timestamp>_add_imported_at_to_files.sql`:

```sql
ALTER TABLE files ADD COLUMN imported_at TEXT NOT NULL DEFAULT '';
UPDATE files SET imported_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE imported_at = '';
```

SQLite's `ALTER TABLE ADD COLUMN` rejects non-constant defaults (no `DEFAULT (strftime(...))`
directly, unlike `CREATE TABLE`) — the empty-string-then-backfill two-step works around that. The
empty string never persists past the migration transaction; every row ends up with a real
timestamp. Existing rows get "now" (the migration's run time) as a reasonable fallback, since their
true original import time was never recorded.

### `core/src/db/dao.rs`

`insert_file`'s `INSERT INTO files (...)` gets `imported_at` added to both the column list and the
`VALUES` list, with the value written as `strftime('%Y-%m-%dT%H:%M:%SZ', 'now')` directly in the SQL
— matching the existing convention in this file (`dao.rs:655, 789, 891` already compute timestamps
this way, not by binding a Rust-side value). The `UPDATE files SET size = ?, fingerprint = ? ...`
path (existing file, content changed) is **not** touched, so re-scanning a file never resets its
original `imported_at`.

**Assumption**: `imported_at` is "when this row was created on *this* device," not a
sync-preserved original-import timestamp. A file pushed/pulled to a second read-flow instance gets
that instance's own import time on first write there, not the source's. This matches how
`guid`/`fingerprint` already behave as per-row-per-device concepts. Flag if sync-preserving
semantics are wanted instead — out of scope here.

### `core/src/db/models.rs`

`File` (the read/query struct) gains `pub imported_at: String`. `NewFile` (insert struct) is
**unchanged** — the value is computed in SQL, not passed from Rust.

### `core/src/api.rs`

`File` (REST DTO) gains `#[serde(default)] pub imported_at: String`, populated in the
`From<(DbFile, Vec<ContentTag>)>` impl — same backward-compat pattern already used for
`has_cover`/`archive_path`/`archive_inner_path` (tolerates a client talking to an older remote
server that predates this field; absent → empty string).

### `cosmic/src/aggregator.rs`

`DocumentSource` (line 688) gains `pub imported_at: String`. Both construction sites (line ~772,
~893) populate it from the corresponding `api::File`.

### `cosmic/src/page/document_details.rs`

In the per-source row (`sources_view`, around line 707-744), add a small caption next to the
existing folder-path caption (`.push(text(folder).size(12))`) showing a formatted date, parsed from
the stored RFC3339 string using the `time` crate — already a `read-flow` (cosmic) crate dependency
(`cosmic/Cargo.toml`: `time = { workspace = true, features = ["formatting", "macros", "std"] }`),
not added to `core`. Exact format (e.g. "Added Jul 15, 2026") decided at implementation time,
checking for any existing date-formatting helper in `cosmic/src` to reuse before writing a new one.

### Scope

**COSMIC only.** `pwa/src/lib/api/client.ts`'s `File`-shaped interface doesn't mirror
`archive_path`/`archive_inner_path` either — PWA display of `imported_at` is a pre-existing gap,
not newly introduced here, and not addressed in this pass.

### Testing

- New `#[cfg(test)]` coverage in `core/src/db/dao.rs`'s existing test module: inserting a file sets
  a non-empty `imported_at`; re-inserting/updating an existing path (changed size/fingerprint)
  leaves `imported_at` unchanged from the original insert.
- `features/documents_detail_view.feature` gets a new scenario asserting a freshly-scanned
  document's source shows an import timestamp (exact assertion shape — e.g. non-empty/parseable —
  decided at implementation time; BDD scenarios don't assert exact wall-clock values), with a new
  step in `cosmic/src/bdd/steps/documents_detail_view.rs`.

## Out of scope

- PWA display of `imported_at` (pre-existing DTO-parity gap, not newly introduced).
- Preserving `imported_at` across cross-instance sync (push/pull) — resets to local insert time on
  each device, as noted above.
- Auto-popup of the scan report dialog (click-to-view from the footer, per the approved design).
- Exposing `error_details`/richer `ScanSummary` fields anywhere in the PWA's admin scan UI — REST
  clients (PWA included) get the new JSON fields for free since `ScanSummary` is the literal
  response body, but building PWA UI to display them is not part of this pass.
