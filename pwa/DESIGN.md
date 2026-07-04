# PWA Design Decisions

Architecture decisions and library assessments for the Read Flow PWA. For usage,
commands, and project structure see [`README.md`](./README.md); for the feature
catalog see [`FEATURES.toml`](../FEATURES.toml) and the generated
[`FEATURE_MATRIX.md`](../FEATURE_MATRIX.md).

## Overview

A Progressive Web App built with SvelteKit and TypeScript. The PWA runs entirely
in the browser — no dedicated backend. It connects to one or more remote
`read-flow` servers as data sources, is installable as a desktop/mobile app, and
caches the app shell for offline use.

## Tech stack

| Concern | Choice | Rationale |
|---------|--------|-----------|
| Framework | SvelteKit (`adapter-static`) | File-based routing, TypeScript-first, SSR disabled for pure SPA/PWA mode |
| Language | TypeScript | Type safety across the whole app |
| Styling | Tailwind CSS v4 | Utility-first, minimal bundle |
| PDF rendering | `pdfjs-dist` | Well-maintained, standard choice |
| EPUB rendering | `epub.js` (`epubjs`) | See assessment below |
| Local storage | `Dexie.js` (IndexedDB) | See storage strategy |
| PWA plumbing | `vite-plugin-pwa` | Generates service worker + web manifest, integrates with SvelteKit/Vite |
| Fuzzy search | `fuse.js` | Lightweight, no server dependency |

## EPUB library assessment: epub.js vs foliate.js

### epub.js (`epubjs` on npm)
- Mature library, ~7k stars, published to npm with TypeScript types (`@types/epubjs`)
- CFI-based location tracking (EPUB Canonical Fragment Identifier — standard, serializable)
- iframe-based rendering (good CSS isolation per book)
- Pagination, theming, search, annotations
- Large community, extensive documentation and examples
- Integrates cleanly into Vite/SvelteKit builds

### foliate.js (GitHub: johnfactotum/foliate-js)
- Modern ESM modules; native DOM rendering (no iframe)
- Better EPUB 3 support, lighter weight
- **Not published to npm** — no TypeScript types, no npm install
- Primarily targeting the Foliate GTK app's embedded WebView; browser compatibility
  in a Vite/SvelteKit context is less proven
- Smaller community, fewer examples

### Decision: epub.js

- npm-installable with TypeScript types — first-class Vite/SvelteKit integration
- CFI-based progress tracking enables precise, serializable position storage that
  matches what the COSMIC app needs for seamless progress sync
- Battle-tested in browser-based EPUB readers

Re-evaluate if EPUB 3 rendering quality becomes a pain point in practice.

## Storage strategy: IndexedDB via Dexie.js (no SQLite)

Browser SQLite (via WASM, e.g. wa-sqlite) adds ≥2 MB to the bundle, requires the
Origin Private File System API (not universally available on all targets), and
introduces WASM complexity for no real gain here. Documents themselves live on
remote servers and are never stored locally in bulk.

**Use IndexedDB via Dexie.js:**
- Native browser API, zero WASM overhead, ~20 KB library
- Async, queryable, persistent across sessions
- Accessible from service workers (needed for offline progress sync)
- Clean TypeScript API with generics and typed tables

**Security note:** `userId` and `passphrase` are stored in IndexedDB
(origin-sandboxed), not unscoped localStorage. Credentials are not encrypted at
rest — acceptable trade-off for a local-use tool.

## Architecture

```
┌──────────────────────────────────────────────────────────┐
│                         PWA                               │
│                                                           │
│  ┌─────────────────────┐   ┌──────────────────────────┐  │
│  │   SvelteKit UI       │   │   Service Worker          │  │
│  │   (SPA / adapter-    │   │   Pre-cache app shell;    │  │
│  │    static)           │   │   network-first for API   │  │
│  └──────────┬───────────┘   └──────────────────────────┘  │
│             │                                              │
│  ┌──────────▼───────────────────────────────────────────┐ │
│  │               Source Aggregator                        │ │
│  │  • Fetches documents from N sources in parallel        │ │
│  │  • Deduplicates by fingerprint (content hash)          │ │
│  │  • Fans out tag/progress writes to all sources         │ │
│  │  • Newest last_updated wins for reading progress       │ │
│  └──────────┬───────────────────────────────────────────┘ │
│             │                                              │
│  ┌──────────▼───────────────────────────────────────────┐ │
│  │           IndexedDB (Dexie.js)                         │ │
│  │   sources | readingProgress | preferences              │ │
│  └──────────────────────────────────────────────────────┘ │
└─────────────────────────┬────────────────────────────────┘
                          │  HTTP — Basic Auth, CORS
              ┌───────────▼─────────┐  ┌──────────────────┐
              │  read-flow server A  │  │ read-flow server B│
              └──────────────────────┘  └──────────────────┘
```

The PWA does **not** scan the filesystem itself. All documents are discovered via
the REST API on each configured remote source.

## Reading progress sync

Progress is keyed by `fingerprint` (SHA-256 of file content — identical for the
same file on different sources and on the desktop app).

**On open:** fetch progress from all sources in parallel, pick the entry with the
newest `last_updated`, seed the viewer there.

**On update (debounced):** write to the IndexedDB cache immediately, then fan out
to all sources in parallel (fire-and-forget; failures are logged but do not block
the reader).

**Format compatibility with the COSMIC app:** the `progress` field is a plain
string stored as-is by the server; both clients must agree on the encoding. The
COSMIC EPUB viewer's serialization (`cosmic/src/page/epub_viewer/`) is the
reference. PDF progress is the page number as a decimal string (matching
`mu_pdf_viewer.rs`).

## Theming: light / dark / system

Three theme modes, default **System** (follows `prefers-color-scheme` live).

- **No-FOUC inline script** in `app.html` runs before any CSS is parsed: reads
  `localStorage` and applies the `dark` class to `<html>` synchronously, so the
  correct scheme is in place before first paint.
- **Tailwind v4 custom variant** — `@custom-variant dark (&:where(.dark, .dark *));`
  makes `dark:` utilities respond to a `.dark` ancestor instead of the media
  query, letting the JS toggle override the OS preference.
- **Persistence uses `localStorage`** (key `read-flow-theme`), not IndexedDB,
  specifically because theme application must be synchronous before first render.
  Absence of the key means "system".
- The reader shell (`/read/*`) intentionally stays dark regardless of theme —
  most reading environments are dim.

## Responsive design principles

Mobile-first with Tailwind's default breakpoint scale (`sm` 640 / `md` 768 /
`lg` 1024 / `xl` 1280 / `2xl` 1536). Key rules:

- Content capped at `max-w-screen-2xl` on very wide displays.
- Mobile navigation: top bar + bottom tab bar with `env(safe-area-inset-bottom)`;
  `md+` uses a persistent left sidebar.
- All interactive elements meet a 44×44 px minimum tap target on mobile
  (WCAG 2.5.5 / Apple HIG), via padding utilities.
- Base font size 16px everywhere — prevents iOS Safari zoom on focused inputs.
  Reading views have a user-adjustable font size stored in preferences.
- No JavaScript media-query logic unless strictly necessary.
