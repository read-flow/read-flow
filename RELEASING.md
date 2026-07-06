# Releasing Read Flow

This document describes how a release is cut. It is an **initial draft** — several points
marked _(decide)_ are open for discussion; refine this file as the process settles.

## Versioning

- The **product version** is what users see and what we tag (`vX.Y.Z`). It tracks the
  `read-flow` desktop application.
- Read Flow follows [Semantic Versioning](https://semver.org): `MAJOR.MINOR.PATCH`.
  While pre-1.0, breaking changes may land in `MINOR` releases.
- **Unified version.** All workspace crates share a single version through
  `[workspace.package] version` in the root `Cargo.toml`; each crate sets
  `version.workspace = true`. Bump the version in **one place** (root `Cargo.toml`).
- Two values are **not** workspace-inheritable and must be bumped by hand to match:
  - `pwa/package.json` `version`.
  - `cosmic/Cargo.toml` `[package.metadata.bundle] version` and the `CFBundleVersion` /
    `CFBundleShortVersionString` strings in the `justfile` macOS bundle recipe.

## Release channels / artifacts

The [`release.yml`](.github/workflows/release.yml) workflow builds these automatically from a
Git tag (`vX.Y.Z`) and attaches them to a **draft** GitHub Release:

- **Linux x86_64**: a portable `.tar.gz` (binary + `.desktop`/icon/metainfo + README/LICENSE/NOTICE)
  **and** a `.deb` package (`cargo-deb`).
- **macOS arm64** (Apple Silicon): a zipped `.app` bundle. **Unsigned** — users bypass Gatekeeper on
  first launch (documented in the README). _(decide: signing + notarization — see open questions.)_
- **Checksums**: `SHA256SUMS` covering every artifact, generated in the workflow.

The **PWA** is not shipped as a separate artifact: the packaging recipes (`just deb`, `just bundle`)
build it (`just pwa-build`) and **embed** it into the `read-flow` binary via the `embed-pwa` feature,
so the server hosts the web UI at its own address (same origin — no CORS/HTTPS gymnastics).

Not built by the workflow (yet):

- Additional targets/formats: macOS Intel, Linux arm64, AppImage, Flatpak — _(decide)_.
- A separately hosted PWA (e.g. GitHub Pages) — possible later, but requires users to expose their
  server over trusted HTTPS; the embedded copy is the primary path.

## Release procedure

### 1. Prepare

1. Make sure `main` is green in CI and the working tree is clean.
2. Decide the new version `X.Y.Z`.
3. Bump versions:
   - `[workspace.package] version` in the root `Cargo.toml` (covers all crates).
   - `pwa/package.json` `version`.
   - `cosmic/Cargo.toml` `[package.metadata.bundle] version` and the `CFBundle*` strings in the
     `justfile` macOS bundle recipe.
4. Update **[CHANGELOG.md](CHANGELOG.md)**:
   - Move `[Unreleased]` entries under a new `## [X.Y.Z] - YYYY-MM-DD` heading.
   - Add fresh empty `Added / Changed / Fixed` subsections to `[Unreleased]`.
   - Update the compare/link references at the bottom.
5. `cargo +nightly fmt`, `cargo nextest run`, and `cd pwa && npm test` — all must pass.

### 2. Commit and tag

```bash
git add -A
git commit -m "release: vX.Y.Z"
git tag -a vX.Y.Z -m "Read Flow vX.Y.Z"
git push github main
git push github vX.Y.Z          # pushing the tag triggers the release workflow
```

### 3. Publish

- The [`release.yml`](.github/workflows/release.yml) workflow runs on the `vX.Y.Z` tag:
  it builds the Linux and macOS artifacts and creates a **draft** GitHub Release with the
  changelog section as the body.
- Review the draft release, confirm artifacts and notes, then **publish** it.

### 4. After release

- Announce _(decide: where — README badge, discussions, elsewhere?)_.
- Verify the `[Unreleased]` section and version numbers are ready for the next cycle.

## Prerequisites for the release manager

- `cargo install cargo-deb` (Linux `.deb`).
- `just` (`cargo install just`) for the packaging recipes.
- macOS with `rsvg-convert` (`brew install librsvg`) for the `.app` icon, if building locally.
- Push access to the GitHub repository.

## Open questions to iterate on

- [x] Version unification (`[workspace.package]`) — **done**; single version in root `Cargo.toml`.
- [x] Prebuilt binaries — **yes**, automated in `release.yml` (Linux x86_64 `.deb`+tarball, macOS arm64 `.app`).
- [x] Linux artifact set — **deb + portable tarball** for now.
- [ ] macOS signing & notarization (currently unsigned; README documents the Gatekeeper workaround).
- [x] PWA hosting — **embedded in the server** (`embed-pwa` feature); packaged builds serve it at `/`.
- [ ] More targets/formats: macOS Intel, Linux arm64, AppImage, Flatpak.
- [ ] Optionally also host the PWA standalone (GitHub Pages) — needs server HTTPS; deferred.
- [ ] Changelog automation (e.g. `git-cliff`) vs. hand-maintained.
- [ ] Publishing library crates (`read-flow-core`, `provider`, `epub`) to crates.io.
