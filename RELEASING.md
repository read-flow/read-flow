# Releasing Read Flow

This document describes how a release is cut. It is an **initial draft** — several points
marked _(decide)_ are open for discussion; refine this file as the process settles.

## Versioning

- The **product version** is what users see and what we tag (`vX.Y.Z`). It tracks the
  `read-flow` desktop application.
- Read Flow follows [Semantic Versioning](https://semver.org): `MAJOR.MINOR.PATCH`.
  While pre-1.0, breaking changes may land in `MINOR` releases.
- _(decide)_ **Single vs. per-crate versions.** Today each workspace crate has its own
  `version` (all `0.1.0`, except the PWA at `0.0.1`). Options to brainstorm:
  1. Unify everything under one version via `[workspace.package] version = "..."` and
     `version.workspace = true` in each crate (simplest to reason about).
  2. Keep library crates (`read-flow-core`, `provider`, `epub`) independently versioned for
     potential separate publishing, and only the product version is tagged.
  For 0.1.0 we align every crate (incl. the PWA) to `0.1.0` and tag `v0.1.0`.

## Release channels / artifacts

For each release we intend to publish, from a Git tag:

- **Linux**: a `.deb` package (`just deb`, via `cargo-deb`). _(decide: also a portable tarball / AppImage?)_
- **macOS**: a `.app` bundle, zipped (`just bundle` → zip `target/release/Read Flow.app`).
  _(decide: code signing + notarization — required for Gatekeeper-friendly distribution.)_
- **PWA**: static build (`just pwa-build`). _(decide: where is it hosted? GitHub Pages / Netlify /
  bundled with the server? Out of scope for the binary release for now.)_
- **Checksums**: `SHA256SUMS` for all uploaded artifacts. _(TODO in the workflow.)_

## Release procedure

### 1. Prepare

1. Make sure `master` is green in CI and the working tree is clean.
2. Decide the new version `X.Y.Z`.
3. Bump versions:
   - `read-flow` (`cosmic/Cargo.toml`) and the other crates per the versioning decision above.
   - `pwa/package.json`.
   - macOS bundle version strings are read from the crate version in the `justfile` — verify.
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
git push github master
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

- [ ] Version unification (`[workspace.package]`) — yes/no.
- [ ] macOS signing & notarization.
- [ ] Linux artifact set (deb only, or + AppImage/tarball/Flatpak).
- [ ] PWA hosting and its release cadence relative to the app.
- [ ] Prebuilt binaries vs. build-from-source expectation for 0.1.0.
- [ ] Changelog automation (e.g. `git-cliff`) vs. hand-maintained.
- [ ] Publishing library crates (`read-flow-core`, `provider`, `epub`) to crates.io.
