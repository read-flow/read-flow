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

- **Linux x86_64 / arm64**: a portable `.tar.gz` (binary + `.desktop`/icon/metainfo +
  README/LICENSE/NOTICE) **and** a `.deb` package (`cargo-deb`) for each arch. The arm64 build
  runs natively on GitHub's `ubuntu-24.04-arm` runner (no cross-compilation) and targets e.g.
  Raspberry Pi 5.
- **macOS arm64** (Apple Silicon): a zipped `.app` bundle. **Unsigned** — users bypass Gatekeeper on
  first launch (documented in the README). _(decide: signing + notarization — see open questions.)_
- **Flatpak**: a single-file `.flatpak` bundle, built from
  [`flatpak/io.github.read-flow.yml`](flatpak/io.github.read-flow.yml) — confirmed green
  end-to-end on v0.1.1 (see "Application stores" below).
- **Checksums**: `SHA256SUMS` covering every artifact, generated in the workflow.

The **PWA** is not shipped as a separate artifact: the packaging recipes (`just deb`, `just bundle`)
build it (`just pwa-build`) and **embed** it into the `read-flow` binary via the `embed-pwa` feature,
so the server hosts the web UI at its own address (same origin — no CORS/HTTPS gymnastics).

Not built by the workflow (bumped by hand instead — see "Application stores"):

- The Homebrew Cask (`Casks/read-flow.rb` in `read-flow/homebrew-read-flow`).
- macOS Intel — no current plan. AppImage and Snap — decided against for now (see "Application
  stores").
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
- [x] Prebuilt binaries — **yes**, automated in `release.yml` (Linux x86_64/arm64 `.deb`+tarball, macOS arm64 `.app`).
- [x] Linux artifact set — **deb + portable tarball** for now.
- [ ] macOS signing & notarization (currently unsigned; README documents the Gatekeeper workaround).
  Needs a paid Apple Developer Program membership before `notarytool` can be wired into
  `release.yml`. Matters for Homebrew Cask trust too, not just direct downloads.
- [x] PWA hosting — **embedded in the server** (`embed-pwa` feature); packaged builds serve it at `/`.
- [x] More targets/formats — **decided**: Flathub and a personal Homebrew tap are the priorities
  (see "Application stores" below); AppImage and Snap are deprioritized/skipped for now; macOS
  Intel stays open (no current plan).
- [ ] Optionally also host the PWA standalone (GitHub Pages) — needs server HTTPS; deferred.
- [x] Changelog automation — **decided**: stay hand-maintained (solo maintainer; `git-cliff` adds
  ceremony without enough payoff at this scale).
- [ ] Publishing library crates (`read-flow-core`, `provider`, `epub`) to crates.io — low priority,
  not required for app-store distribution; revisit if/when `provider`/`epub` get extracted to their
  own repos.
- [ ] Announce channel (README badge, GitHub Discussions, elsewhere) — deferred, low stakes.

## Application stores

Priority order, decided 2026-07-10:

1. **Flathub** — best fit technically: Flatpak's portal-based file access (native folder picker
   crossing the sandbox) matches Read Flow's "scan user-configured directories" model better than
   Snap's static plugs. No cost, no publisher account beyond GitHub.
   - Prerequisite (done): app ID renamed `com.github.read-flow.read-flow` →
     `io.github.read-flow` — Flathub requires the `io.github.<owner>.<repo>` convention
     for GitHub-hosted apps; `com.github.*` is not accepted.
   - Manifest: [`flatpak/io.github.read-flow.yml`](flatpak/io.github.read-flow.yml). File-access
     model: `--filesystem=home:rw` for the first submission (simple, well-precedented for
     file-manager-style apps) rather than `xdg-desktop-portal` folder grants (correct sandboxing,
     but needs the `ashpd` crate + UI changes — real dev work, revisit later if Flathub reviewers
     push back).
   - CI: `.github/workflows/release.yml` job `build-flatpak` builds a `.flatpak` bundle on every
     tagged release and attaches it to the GitHub Release, using the official
     `flatpak/flatpak-github-actions` action + `flatpak-builder-tools` generators for offline
     cargo/npm sources. **Confirmed working**: green end-to-end on v0.1.1, including mupdf's full
     C/C++ compile and the entire libcosmic/wgpu/sqlx stack (run 29185080376).
   - Standalone submission repo: [`read-flow/io.github.read-flow`](https://github.com/read-flow/io.github.read-flow)
     — the manifest adapted for Flathub's build infrastructure (`type: git` source pinned to a
     release tag/commit instead of the local checkout CI uses), with `cargo-sources.json` /
     `node-sources.json` committed as static files (Flathub can't run our CI's generation step).
     Same build logic as the CI-verified manifest above.
   - **Still open**: submitting to Flathub itself (a PR against `github.com/flathub/flathub`
     pointing at the submission repo) and a Linux-side `flatpak-builder` run of that repo — neither
     done yet, both straightforward from here.
2. **Homebrew**: own tap, [`read-flow/homebrew-read-flow`](https://github.com/read-flow/homebrew-read-flow)
   — not homebrew-core (which requires "notability" — stars/forks this project doesn't have yet).
   Ships a Cask (`Casks/read-flow.rb`) for the macOS `.app`, installing via
   `brew install --cask read-flow/read-flow/read-flow`. **Confirmed working**: `brew style`/`brew
   audit --online` clean, `brew install` verified the download/sha256 and resolved to the correct
   `/Applications/Read Flow.app` path. No Formula for `read-flow --headless` server use — the
   binary needs the full GUI dependency tree either way, so a Formula would just be a heavier
   build-from-source path to the same thing the Cask already covers.
3. **AUR** — near-zero-effort bonus alongside Flathub: a `PKGBUILD` in a personal repo, no review
   process, can reuse the same build steps as `just deb`.
4. **Snap Store** — deprioritized. Mechanically similar effort to the existing `.deb` (same system
   deps), but strict confinement fights the arbitrary-directory-scanning use case (`home`/
   `removable-media` plugs, or Canonical manual review for broader access). Revisit if Flathub
   traction doesn't pan out.
