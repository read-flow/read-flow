# Read Flow

**Organize and read your e-book and PDF library across all your machines — locally, privately, and on your terms.**

Read Flow scans your document folders, fingerprints and de-duplicates what it finds, lets you
tag and track your reading, and reads your books for you in a built-in PDF and EPUB viewer.
Everything runs on your own hardware; there is no cloud account and nothing leaves your network
unless you ask it to.

> **Status:** early release (`0.1.0`). Prebuilt binaries for Linux (x86_64) and macOS (Apple
> Silicon) are published on the [Releases](https://github.com/peterpaul/read-flow/releases) page;
> you can also build from source.

## Screenshots

<!-- screenshot: desktop app — library / dashboard view -->
<!-- screenshot: desktop app — EPUB reader -->
<!-- screenshot: desktop app — PDF reader -->
<!-- screenshot: web app (PWA) — library with fuzzy search -->
<!-- screenshot: online library (OPDS) search -->

_Screenshots coming soon._

## Features

- **Automatic scanning** of your document folders, with content-based (SHA-256) fingerprinting
  and **duplicate detection**.
- **Tags & auto-tagging** — organize your library with tags, and define rules that tag documents
  automatically as they are discovered.
- **Reading status & progress** — mark documents Unread / Reading / Read, and pick up where you
  left off, kept in sync across your devices.
- **Built-in readers** — a PDF viewer and a native EPUB reader render your books directly in the app.
- **Online libraries (OPDS)** — search and pull public-domain titles from catalogs such as
  [Project Gutenberg](https://www.gutenberg.org) and [Standard Ebooks](https://standardebooks.org).
- **Fuzzy search** in the web app — find what you want even with typos or partial titles.
- **Private mode** — hide sensitive documents behind a private-tag filter.
- **Multiple languages** — the desktop app is translated into English, French, and Dutch.

**Supported formats:** PDF, EPUB, MOBI, FB2, CBZ/CBT (comics), DOCX/XLSX/PPTX/XPS, and documents
found inside archive files.

## How you use it

Read Flow gives you three ways in, all backed by the same library:

1. **Desktop app** (`read-flow`) — a native application for **Linux and macOS**. This is the main
   way to use Read Flow: browse your library, read, tag, and manage scan folders from a GUI.
2. **Headless server** (`read-flow --headless`) — run the same app without a UI on a home server or
   NAS, so the web app can connect to it over your network.
3. **Web app (PWA)** — a browser-based reader with fuzzy search and offline-capable reading. The
   server hosts it at its own address, so you just open the server URL in a browser (no separate
   deployment), and install it to your device like a native app. It can aggregate multiple Read
   Flow servers as sources.

## Install

Grab the latest build for your platform from the
[Releases](https://github.com/peterpaul/read-flow/releases) page.

**Linux (x86_64)**

- **`.deb`** (Debian/Ubuntu): `sudo apt install ./read-flow_*.deb`
- **Portable tarball**: extract `read-flow-*-linux-x86_64.tar.gz` and run the `read-flow` binary
  inside. (Optionally copy the `.desktop` and `.svg` files into `~/.local/share/`.)

**macOS (Apple Silicon)**

Download `read-flow-*-macos-arm64.zip`, unzip it, and move **Read Flow.app** to `/Applications`.

> The app is **not code-signed** yet, so macOS Gatekeeper blocks it on first launch. To open it:
> right-click **Read Flow.app** → **Open** → **Open**. Alternatively, from a terminal:
> `xattr -dr com.apple.quarantine "/Applications/Read Flow.app"`.

Verify a download against `SHA256SUMS` from the release: `shasum -a 256 -c SHA256SUMS`.

## Build from source

You'll need the [Rust toolchain](https://rustup.rs/) and, for the web app, [Node.js](https://nodejs.org/) ≥ 20.
See [CONTRIBUTING.md](CONTRIBUTING.md) for the full list of prerequisites (including the Linux system
libraries the desktop app needs).

**Run the desktop app:**

```bash
cargo run -p read-flow --release
```

Then add one or more folders to scan from the app's settings, and let Read Flow index your documents.

**Run a headless server** (e.g. on a home server) that also serves the web app at the same address:

```bash
# On the server machine — builds the PWA, embeds it, and serves API + web UI together:
just serve --address 0.0.0.0 --port 8000
```

Then open `http://<server>:8000` in a browser and install the PWA from there. (Packaged release
builds already include the embedded web app, so `read-flow --headless` serves it directly.)

## Configuration

Runtime settings live in `read-flow.toml` at the workspace root. Every setting also has a control
in the app's Preferences UI, so you rarely need to edit the file by hand. The main sections are:

| Section            | What it controls                                                       |
| ------------------ | ---------------------------------------------------------------------- |
| `[database]`       | Path to the SQLite database file.                                      |
| `[server]`         | Bind address/port, allowed origins, and authorized users.             |
| `[scan]`           | Which folders to scan, file types, concurrency, auto-tag rules.        |
| `[ui]`             | Private mode and which tags count as private.                         |
| `[online_library]` | OPDS catalogs to search (Project Gutenberg, Standard Ebooks, …).       |

> `read-flow.toml` is **git-ignored** because it contains your server's hashed credentials and
> machine-specific paths. Each user creates their own; the app writes one for you on first run.

---

## For contributors

Read Flow is a Rust [workspace](https://doc.rust-lang.org/cargo/reference/workspaces.html)
(edition 2024) plus a SvelteKit web app.

### Architecture

| Crate / package      | Role                                                                             | License            |
| -------------------- | -------------------------------------------------------------------------------- | ------------------ |
| `cosmic/` (`read-flow`) | Desktop app (libcosmic GUI) **and** the headless HTTP server. PDF + EPUB readers. | GPL-3.0-or-later   |
| `core/` (`read-flow-core`) | Core library: async SQLite (sqlx), file scanning, tagging, Axum REST API.   | MIT                |
| `provider/`          | Small dependency-injection / observable-cache library used across the app.       | MIT                |
| `epub/`              | EPUB3 parsing and rendering (container, nav, HTML→content, CSS, images).         | MIT                |
| `widgets/` (`read-flow-widgets`) | Shared desktop UI widgets.                                            | GPL-3.0-or-later   |
| `pwa/` (`read-flow-pwa`) | SvelteKit + TypeScript web app; talks to servers over REST.                  | MIT                |

Dependency flow:

```
read-flow (cosmic) ──> read-flow-core ──> provider
       │
   widgets, epub

pwa  ──(REST API)──>  read-flow-core server
```

The web app is independent of the Rust crates and communicates only over the REST API.

> There is also a small `read-flow-cli` binary inside `core/`. It exists **only** as the
> server-launcher for the integration and end-to-end test harness — it is not a user-facing tool.
> To run a headless server, use `read-flow --headless`.

### Prerequisites

- **Rust** (stable, edition 2024) via [rustup](https://rustup.rs/); the **nightly** toolchain is
  needed for formatting (`cargo +nightly fmt`).
- [`cargo-nextest`](https://nexte.st/): `cargo install cargo-nextest`.
- **Node.js ≥ 20** and **npm ≥ 10** for the web app.
- On **Linux**, the system libraries required by [libcosmic](https://github.com/pop-os/libcosmic)
  (Wayland, `wgpu`/GPU stack, `libxkbcommon`, `libudev`, …) and SQLite.

See [CONTRIBUTING.md](CONTRIBUTING.md) for step-by-step setup, build, run, and test instructions.

### Build & test

```bash
cargo build                 # build the whole workspace
cargo nextest run           # run the Rust test suite
cargo +nightly fmt          # format (run before committing)

cd pwa && npm install && npm test   # web app unit tests
```

---

## License

Read Flow is released under a **split license** (see [`NOTICE`](NOTICE) for the full breakdown):

- The **desktop application** (`read-flow`) and `read-flow-widgets` are licensed under the
  **GNU General Public License v3.0 or later** ([`LICENSE-GPL`](LICENSE-GPL)). The PDF viewer is
  derived from [COSMIC Reader](https://github.com/pop-os/cosmic-reader) by System76 (GPL-3.0-or-later),
  which is why the app carries this license.
- The **library crates** (`read-flow-core`, `provider`, `epub`) and the **web app**
  (`read-flow-pwa`) are licensed under the **MIT License** ([`LICENSE-MIT`](LICENSE-MIT)).

## Development & AI transparency

<!-- TODO (author): review and adjust the wording/tools below before publishing — this is your personal note. -->

Read Flow started as a project I wrote by hand, including its first user interface built with
[iced](https://iced.rs/). As development continued, I began experimenting with AI coding tools —
GitHub Copilot, the Cursor editor, and others (including Claude Code) — especially while building
the current desktop UI on [libcosmic](https://github.com/pop-os/libcosmic). AI assistance has been
part of the workflow since then; all code is reviewed and integrated by me.

## Author

Peterpaul Klein Haneveld — <pp.kleinhaneveld@gmail.com>
