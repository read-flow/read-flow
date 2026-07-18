# Online library catalogs section in Preferences

## Context

There is currently no user-facing way to add a custom OPDS catalog, or to disable a built-in one
(Project Gutenberg, Standard Ebooks) — the only way is hand-editing `[[online_library.catalogs]]`
entries in `read-flow.toml`, in the tagged `Catalog` format introduced by the recent
built-in-catalog refactor (see `CHANGELOG.md`'s `[Unreleased]` breaking-change entry). This spec
adds a "Online Library" section to COSMIC Preferences so catalogs can be managed from the UI,
following the same list-with-inline-form pattern already used for authorized users
(`cosmic/src/forms/settings/authorized_user.rs` + `preferences.rs`'s `authorized_users_section`).

The Online Library page's catalog list (`OnlineLibraryPage`/`CatalogsProvider`) already reloads
automatically whenever settings change (see the prior `CatalogsProvider`/`settings_invalidation_subscription`
work) — so saving a change here will be reflected there with no additional wiring.

## Data model

No core changes needed. `core/src/online_library/mod.rs` already has everything required:

- `Catalog::Builtin(BuiltinCatalog { id: BuiltinCatalogId, enabled: bool })` — code owns name/URL.
- `Catalog::Configured(ConfiguredCatalog { name: String, search_url: String, enabled: bool })` —
  user-owned custom catalogs.
- `BuiltinCatalogId::iter()` (via `strum::EnumIter`) — `ProjectGutenberg`, `StandardEbooks`.
- `BuiltinCatalogId::display_name()` / `Catalog::resolve()` / `Catalog::enabled()`.

`self.settings.online_library.catalogs: Vec<Catalog>` on `PreferencesPage` (already present as part
of the full `Settings` struct it holds) is mutated directly by the handlers below, following the
same in-place-draft-then-Save convention every other preferences field uses
(`self.save_state = SaveState::Idle` marks the draft dirty; the existing Save button persists via
`settings.save(path)` and resets `save_state`).

## New file: `cosmic/src/forms/settings/catalog.rs`

Mirrors `AuthorizedUserForm` exactly in shape:

```rust
pub struct CatalogForm {
    original_name: Option<String>,   // None = adding; Some(name) = editing (identity while editing)
    editing_name: String,
    editing_search_url: String,
}

pub enum CatalogFormMessage {
    EditName(String),
    EditSearchUrl(String),
    Out(CatalogFormOutput),
}

pub enum CatalogFormOutput {
    /// original_name, name, search_url
    Submit(Option<String>, String, String),
    Cancel,
}
```

- `CatalogForm::new(existing: Option<(String, String)>) -> (Self, Task<Action<CatalogFormMessage>>)`
  — `Some((name, search_url))` prefills for editing; `None` starts a blank add form. Returns
  `task::none()` like `AuthorizedUserForm::new` (no async init work needed).
- `is_submittable(&self, other_names: &[String]) -> bool` — `editing_name` and `editing_search_url`
  both non-empty, **and** `editing_name` doesn't case-sensitively match any name in `other_names`
  (the caller passes every other catalog's resolved display name — built-in and configured, minus
  the entry currently being edited). This closes the pagination-name-collision class of bug fixed
  earlier in `cosmic/src/page/online_library.rs` at the source: the form is the only place users
  can create a new catalog identity, so it's the right place to guarantee uniqueness up front.
- `view()` — two `widget::settings::item::builder` rows (name text input, search_url text input)
  + cancel/submit icon buttons, same layout as `AuthorizedUserForm::view()`.
- `update()` — same shape as `AuthorizedUserForm::update()`; `Out(_)` variant panics with the same
  "should be handled by the parent component" message (parent-handled contract).

Exported via `cosmic/src/forms/settings/mod.rs` (wherever `authorized_user` is currently declared).

## `cosmic/src/page/preferences.rs` changes

**`PreferencesSection`**: add `OnlineLibrary` variant.

**`PreferencesPage` struct**: add `catalog_form: Option<CatalogForm>` field (initialized to `None`
in `new()`, alongside `authorized_user_form: None`). `fn can_be_saved(&self) -> bool` (preferences.rs:449)
blocks the Save button while any inline edit form is open
(`self.authorized_user_form.is_none() && self.directory_settings_form.is_none()`) — extend it with
`&& self.catalog_form.is_none()`.

**`PreferencesMessage`**: add variants
```rust
ToggleBuiltinCatalog(BuiltinCatalogId, bool),
AddCatalog,
EditCatalog(String),           // by current display name
DeleteCatalog(String),         // by current display name
ToggleConfiguredCatalog(String, bool),
CatalogForm(CatalogFormMessage),
```
plus `impl From<CatalogFormMessage> for PreferencesMessage` (`Self::CatalogForm`), matching
`AuthorizedUserFormMessage`'s `From` impl.

**`view_overview()`**: add a card tuple
`(PreferencesSection::OnlineLibrary, fl!("preferences-online-library-section"), fl!("preferences-online-library-section-description"), "system-search-symbolic")`
(same icon as the Online Library page itself) to the existing array-then-map.

**`view()`**'s section dispatch `match`: add
`PreferencesSection::OnlineLibrary => self.view_section_online_library(),`.

**New `fn view_section_online_library(&self) -> Vec<Element<'_, PreferencesMessage>>`**:

- Built-in catalogs subsection: fold over `BuiltinCatalogId::iter()`, for each id look up the
  matching `Catalog::Builtin` entry in `self.settings.online_library.catalogs` (`.find(...)`); its
  displayed toggle state is `entry.map(|b| b.enabled).unwrap_or(false)` (absent = effectively
  disabled, since `resolve_catalogs`/the search flow never returns/searches it — the toggle must
  reflect actual resolved behavior, not an assumed default). `BuiltinCatalogId::display_name()` is
  private to `core`, so the display name comes from the already-public `id.resolve(enabled).name`
  (cheap: two static `&'static str`s plus a `.to_string()`). Each row: icon, that name, toggler
  wired to `PreferencesMessage::ToggleBuiltinCatalog(id, ..)`. No edit/delete button.
- Custom catalogs subsection: fold over `self.settings.online_library.catalogs.iter()` filtering
  `Catalog::Configured`, rendering each as a row (name, toggler, edit button, delete button — same
  icon names as `view_authorized_user_input`: `edit-symbolic`/`edit-clear-symbolic` while editing,
  `list-remove-symbolic` destructive-class delete), followed by
  `crate::component::section_helpers::section_add_button(fl!("settings-online-library-add-catalog"), Some(PreferencesMessage::AddCatalog))`.
- If `self.catalog_form.is_some()`, push `form.view().map(Into::into)` after the custom-catalogs
  section, same as the authorized-user form's `if let Some(form) = ...` block.

**`update()` handlers**:
- `ToggleBuiltinCatalog(id, enabled)`: find-or-insert the matching `Catalog::Builtin` entry in the
  vec, set its `enabled`; `save_state = SaveState::Idle`; `Task::none()`.
- `AddCatalog`: `let (catalog_form, init) = CatalogForm::new(None); self.catalog_form = Some(catalog_form); init.map(ActionExt::map_into)`.
- `EditCatalog(name)`: find the matching `Configured` entry's `search_url`, `CatalogForm::new(Some((name, search_url)))`, same pattern as `AddCatalog`.
- `DeleteCatalog(name)`: `retain` to remove the matching `Configured` entry by name; if
  `self.catalog_form`'s `original_name` matches, clear the form (mirrors `DeleteAuthorizedUser`'s
  `is_editing_authorized_user` check); `save_state = SaveState::Idle`.
- `ToggleConfiguredCatalog(name, enabled)`: find the matching `Configured` entry, set `enabled`;
  `save_state = SaveState::Idle`.
- `CatalogForm(message)`: match on `CatalogFormMessage::Out(output)` —
  - `Submit(original_name, name, search_url)`: the form only edits name/search_url, not `enabled`,
    so: look up `enabled` from the existing `Configured` entry matching `original_name` (`true` if
    `original_name` is `None`, i.e. adding); remove that existing entry, if any; push a new
    `Catalog::Configured { name, search_url, enabled }` with the looked-up value. Then
    `self.catalog_form = None`; `save_state = SaveState::Idle`.
  - `Cancel`: `self.catalog_form = None`.
  - other variants: delegate to `self.catalog_form.as_mut().map(|f| f.update(message))`, same
    `_ => match self.catalog_form.as_mut() { ... }` shape as the authorized-user form.

## Bookkeeping

**`FEATURES.toml`** — new entry under the "Online library" section:
```toml
[[feature]]
id = "online_library.manage_catalogs"
description = "Add/edit/remove custom OPDS catalogs and toggle built-ins, from Preferences"
surfaces = ["cosmic"]
gaps = ["pwa", "rest"]
```
(No REST endpoint exists to edit `online_library.catalogs` remotely — out of scope here, same
category as TLS cert paths: COSMIC-local config editing, not exposed to `ServerSettingsDto`.)

**`features/online_library_manage_catalogs.feature`** (tagged `@cosmic` only, following
`app_theme_overrides.feature`'s precedent for COSMIC-only surfaces):
```gherkin
@online_library_manage_catalogs
Feature: Manage online library catalogs
  Built-in catalogs can be toggled on/off; custom OPDS catalogs can be added, edited, and
  removed, all from COSMIC Preferences → Online Library.

  @cosmic
  Scenario: Adding a custom catalog makes it appear in the list
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    When I add a catalog "My Library" with search URL "https://example.com/opds?q={searchTerms}"
    Then "My Library" appears in the list of online library catalogs

  @cosmic
  Scenario: Disabling a built-in catalog removes it from the enabled catalog list
    Given a read-flow server is running with user "alice" and passphrase "correct-horse"
    When I disable the built-in catalog "standard_ebooks"
    Then "Standard Ebooks" no longer appears in the list of enabled online library catalogs
```

**`cosmic/src/bdd/steps/online_library_manage_catalogs.rs`** — new step file, following
`admin_authorized_users.rs`'s precedent exactly: steps call `world.driver.add_catalog(...)` /
`world.driver.disable_builtin_catalog(...)` / `world.driver.catalog_is_listed(...)`, implemented
only on `CosmicDriver` (bypassing the form, calling `application_module.update_settings(...)`
directly — exactly what the real form's `Submit` handler ends up doing, per `add_user`'s own
documented precedent) since this feature is `@cosmic`-only.

**i18n** (`cosmic/i18n/{en,fr,nl}/read-flow.ftl`), new keys following the existing
`preferences-<section>-section[-description]` / `settings-<section>-<thing>` conventions:
`preferences-online-library-section`, `preferences-online-library-section-description`,
`settings-online-library-builtin-catalogs`, `settings-online-library-custom-catalogs`,
`settings-online-library-custom-catalogs-description`, `settings-online-library-add-catalog`,
`settings-online-library-catalog-name`, `settings-online-library-catalog-name-placeholder`,
`settings-online-library-catalog-search-url`, `settings-online-library-catalog-search-url-placeholder`,
`settings-online-library-edit-catalog`, `settings-online-library-add-catalog-title`.

**`CHANGELOG.md`** — new `### Added` entry under `[Unreleased]`: "COSMIC Preferences → Online
Library: add/edit/remove custom OPDS catalogs, and enable/disable the built-in ones (Project
Gutenberg, Standard Ebooks)."

## Testing

- `CatalogForm::is_submittable` gets inline `#[cfg(test)]` unit tests (non-empty validation +
  name-collision rejection), matching the "every non-trivial pure function gets a unit test"
  convention — `AuthorizedUserForm`'s equivalent (`password_meets_requirements`) has no test
  today, but this form's uniqueness check is exactly the kind of logic that previously caused a
  real bug (the pagination name collision), so it earns one here regardless of precedent.
- The two new BDD scenarios cover add + built-in-disable end to end through the same
  `application_module.update_settings` path the real UI uses, consistent with how every other
  preferences-editing feature in this codebase is tested (no literal widget-click GUI automation
  exists anywhere in this suite).
- `cargo nextest run -p read-flow`, `cargo clippy --workspace --all-targets`,
  `cargo +nightly fmt -- --check`, `just bdd cosmic` all must pass before this is considered done.

## Out of scope

- REST/PWA catalog management (tracked as an acknowledged `FEATURES.toml` gap, not built here).
- Reordering catalogs.
- URL format validation beyond non-empty (matches `AuthorizedUserForm`'s minimal-validation
  precedent; OPDS search URLs use a `{searchTerms}` placeholder that isn't valid percent-encoded
  URL syntax, so strict URL parsing would need special-casing that isn't worth it for a first cut).
