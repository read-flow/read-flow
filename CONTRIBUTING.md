# Contributing to Read Flow

Thanks for your interest in Read Flow! This guide covers everything you need to set up a
development environment, build and run the components, and run the tests.

## Prerequisites

### Rust

Install the toolchain via [rustup](https://rustup.rs/). Read Flow uses **edition 2024** (stable),
and the **nightly** toolchain for formatting:

```bash
rustup toolchain install nightly     # for `cargo +nightly fmt`
cargo install cargo-nextest          # test runner used by the project
```

### Node.js (web app only)

The web app in `pwa/` needs **Node.js ≥ 20** and **npm ≥ 10**.

### Linux system libraries (desktop app)

The desktop app is built on [libcosmic](https://github.com/pop-os/libcosmic) and needs the
Wayland / GPU / input stack plus SQLite. On Debian/Ubuntu:

```bash
sudo apt-get install \
  libwayland-dev libxkbcommon-dev libudev-dev libinput-dev \
  libgbm-dev libegl1-mesa-dev libsqlite3-dev pkg-config
```

(Package names vary by distribution; the equivalents for your distro are what you need.)

On **macOS** no extra system packages are required beyond the Rust toolchain.

### Optional speed-ups

For faster iteration, disable LTO in the release profile, install the
[mold](https://github.com/rui314/mold) linker, and configure
[sccache](https://github.com/mozilla/sccache). Configure your editor to use
[rust-analyzer](https://rust-analyzer.github.io/).

## Project layout

| Path         | Package             | What it is                                                    |
| ------------ | ------------------- | ------------------------------------------------------------- |
| `cosmic/`    | `read-flow`         | Desktop app + headless server. PDF/EPUB readers. **GPLv3**.   |
| `core/`      | `read-flow-core`    | Core library: SQLite, scanning, tagging, Axum REST API. MIT.  |
| `provider/`  | `provider`          | Dependency-injection / observable-cache library. MIT.         |
| `epub/`      | `epub`              | EPUB3 parsing and rendering. MIT.                             |
| `widgets/`   | `read-flow-widgets` | Shared desktop UI widgets. **GPLv3**.                         |
| `pwa/`       | `read-flow-pwa`     | SvelteKit + TypeScript web app. MIT.                          |

See [`NOTICE`](NOTICE) for the full licensing breakdown and third-party attribution.

## Build & run

The repository ships a [`justfile`](justfile) ([casey/just](https://github.com/casey/just)) with
convenient recipes, but the raw Cargo/npm commands work everywhere:

```bash
# Build the whole workspace
cargo build                              # or: cargo build --release

# Run the desktop app
cargo run -p read-flow --release            # or: just run

# Run the headless server (no UI)
cargo run -p read-flow --release -- --headless --address 0.0.0.0 --port 8000

# Run the web app dev server
cd pwa && npm install && npm run dev     # or: just pwa-install && just pwa-dev
```

### Database

The app uses **SQLite** via `sqlx` (async, WAL mode, foreign keys on). Migrations live in
`core/migrations/` as `{timestamp}_{description}.sql` files and run automatically at startup —
there is no manual migration step. The database path is configured under `[database]` in
`read-flow.toml`.

### Configuration

Runtime configuration is `read-flow.toml` (workspace root). It is **git-ignored** because it holds
hashed credentials and machine-specific paths; the app creates one for you on first run.

> **Config UI parity:** every setting in `read-flow.toml` must have a matching control in the
> desktop Preferences page (`cosmic/src/page/preferences.rs`) and — for server settings exposed
> over REST — in the web app admin page (`pwa/src/routes/settings/admin/+page.svelte`), with
> i18n strings for en/fr/nl.

## Testing

Read Flow follows **test-driven development**: write a failing test first, make it pass, then
refactor. Bug fixes start with a failing regression test.

```bash
# Rust — whole workspace
cargo nextest run

# Rust — a single crate or a single test
cargo nextest run -p read-flow-core
cargo nextest run -p read-flow-core test_name

# BDD (cucumber) harness, REST or COSMIC driver
just bdd            # BDD_DRIVER=rest (default)
just bdd cosmic

# Web app
cd pwa && npm test              # unit tests (Vitest)
cd pwa && npm run test:e2e      # end-to-end (Playwright/Cucumber, builds first)
```

- Rust tests use [`rstest`](https://docs.rs/rstest) and [`assert4rs`](https://docs.rs/assert4rs) in
  inline `#[cfg(test)]` modules, co-located with the code.
- Web app tests are `*.test.ts` files next to the code they cover.

## Code style

- **Rust formatting:** run `cargo +nightly fmt` before committing. Never commit unformatted code.
  Imports are vertical (one per line), grouped std / external / crate.
- **Functional style:** prefer pure functions, immutable data, and transformation pipelines
  (`map`/`filter`/`fold`) over imperative loops. Keep I/O and DB access at the boundary and the
  logic underneath pure and unit-tested.

## Commits

- Keep commit messages concise and descriptive.
- Do **not** add co-author trailers or mention AI tooling in commit messages.

## Pull requests

1. Fork the repository and create a topic branch.
2. Add or update tests for your change.
3. Run `cargo +nightly fmt`, `cargo nextest run`, and (if you touched the web app) `npm test`.
4. Open a pull request describing the change and why.
