# PWA Design Document

## Overview

Replace the existing Express.js SPA (`spa/`) with a Progressive Web App (PWA) built with
SvelteKit and TypeScript. The PWA runs entirely in the browser — no dedicated backend.
It connects to one or more remote `read-flow` servers as data sources, is installable as a
desktop/mobile app, and caches the app shell for offline use.

---

## Tech Stack

| Concern | Choice | Rationale |
|---------|--------|-----------|
| Framework | SvelteKit (`adapter-static`) | File-based routing, TypeScript-first, SSR disabled for pure SPA/PWA mode |
| Language | TypeScript | Type safety across the whole app |
| Styling | Tailwind CSS v4 | Utility-first, minimal bundle, consistent with existing `spa/` |
| PDF rendering | `pdfjs-dist` | Already used in `spa/`; well-maintained, standard choice |
| EPUB rendering | `epub.js` (`epubjs`) | See assessment below |
| Local storage | `Dexie.js` (IndexedDB) | See storage strategy |
| PWA plumbing | `vite-plugin-pwa` | Generates service worker + web manifest, integrates with SvelteKit/Vite |
| Fuzzy search | `fuse.js` | Already used in `spa/`; lightweight, no server dependency |

---

## EPUB Library Assessment: epub.js vs foliate.js

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

epub.js is the correct choice for this project because:
- npm-installable with TypeScript types — first-class Vite/SvelteKit integration
- CFI-based progress tracking enables precise, serializable position storage that
  matches what the cosmic app needs for seamless progress sync
- Battle-tested in browser-based EPUB readers

Re-evaluate if EPUB 3 rendering quality becomes a pain point in practice.

---

## Storage Strategy: IndexedDB via Dexie.js (no SQLite)

**Answer to open question — do not use SQLite.**

Browser SQLite (via WASM, e.g. wa-sqlite) adds ≥2 MB to the bundle, requires
the Origin Private File System API (not universally available on all targets), and
introduces WASM complexity for no real gain here. Documents themselves live on remote
servers and are never stored locally in bulk.

**Use IndexedDB via Dexie.js:**
- Native browser API, zero WASM overhead, ~20 KB library
- Async, queryable, persistent across sessions
- Accessible from service workers (needed for offline progress sync)
- Clean TypeScript API with generics and typed tables

### Local IndexedDB Stores

| Store | Schema | Notes |
|-------|--------|-------|
| `sources` | `{ id, name, baseUrl, userId, passphrase, order }` | Configured remote servers |
| `readingProgress` | `{ fingerprint, progress, lastUpdated }` | Local cache; synced to remotes |
| `preferences` | `{ key, value }` | UI state (pane width, theme, etc.) |

**Security note:** `userId` and `passphrase` are stored in IndexedDB (origin-sandboxed),
not unscoped localStorage. Risk level matches the existing SPA. Credentials are not
encrypted at rest — acceptable trade-off for a local-use tool.

---

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
`GET /files` on each configured remote source.

---

## Features

### Source Management (`/settings/sources`)
- Add remote read-flow servers: name, base URL, user ID, passphrase
- Test connectivity via `GET /status` before persisting
- Reorder (drag or up/down buttons) and delete sources
- All data stored in IndexedDB `sources` table

### Document Library (`/`)
- Fetch `GET /files` from all sources in parallel on load
- Deduplicate entries by `fingerprint`; track which source(s) each file exists on
- Display: filename, file type, size, directory, tags, reading status
- Fuzzy search with Fuse.js (keys: name, path; threshold: 0.3)
- Tag filter panel: allow (AND) / deny (NOT) logic — matches cosmic app behavior
- Virtual/paginated list for large collections
- Collapsible details sidebar (width persisted in IndexedDB preferences)

### Tag Management
- Add/remove tags in the details sidebar
- Write fans out to **all** sources that have the file (by fingerprint)
- Tag autocomplete drawn from merged tag list across all sources

### PDF Viewer (`/read/pdf/[fingerprint]`)
- Render with pdf.js inside a dedicated route
- Track progress as current page number
- Sync progress to all sources on page change (debounced)
- Page and zoom preference stored in IndexedDB

### EPUB Viewer (`/read/epub/[fingerprint]`)
- Render with epub.js in an iframe-backed component
- CFI-based location tracking
- Sync CFI progress to all sources on location change
- Font size, theme, and layout preferences stored in IndexedDB

### Reading Progress Sync

Progress is keyed by `fingerprint` (SHA-256 of file content — identical for the same
file on different sources and on the desktop app).

**On open:**
1. Fetch `GET /reading-progress/<fingerprint>` from all sources in parallel
2. Pick the entry with the newest `last_updated` timestamp
3. Seed the viewer at that location

**On progress update (debounced):**
1. Write to IndexedDB local cache immediately
2. Fan out `PUT /reading-progress` to all sources in parallel (fire-and-forget;
   failures are logged but do not block the reader)

**Format compatibility with the cosmic app:**
The `progress` field is a plain string stored as-is by the server. The cosmic EPUB
viewer and the PWA must agree on format. During implementation, inspect
`cosmic/src/page/epub_viewer/mod.rs` for the exact serialization before finalising the
epub.js CFI → progress string encoding. PDF progress uses page number as a decimal
string (to match `mu_pdf_viewer.rs` behavior).

### PWA Installation
- `manifest.json`: name="Read Flow", display=standalone, icons at 192×192 and 512×512
- Service worker via vite-plugin-pwa: pre-caches app shell assets; API calls use
  network-first strategy (documents must come from live servers)
- Installable on Chrome/Edge desktop and iOS/Android Safari

---

## Responsive Design

The app must work reliably across the full device spectrum: a 375px-wide mobile phone
in portrait, a tablet in either orientation, a typical 1280–1920px laptop/desktop, and
a 4K fullscreen browser window. Tailwind's breakpoint utilities are the primary tool;
no JavaScript media-query logic unless strictly necessary.

### Breakpoints

Tailwind's default scale maps cleanly to the target form factors:

| Breakpoint | Min width | Target |
|------------|-----------|--------|
| _(base)_   | 0 px      | Portrait phone (375–430 px) |
| `sm`       | 640 px    | Landscape phone, small tablet |
| `md`       | 768 px    | Tablet portrait |
| `lg`       | 1024 px   | Tablet landscape, small laptop |
| `xl`       | 1280 px   | Desktop |
| `2xl`      | 1536 px   | Large desktop, 4K |

All layouts are designed **mobile-first**: the base styles define the mobile layout,
and breakpoint prefixes progressively enhance for wider viewports.

### Navigation Shell

```
Mobile (< md)                   md +
┌────────────────────┐          ┌─────┬──────────────────────┐
│ ☰  Read Flow   🔍  │          │     │                      │
├────────────────────┤          │ Nav │   Page content       │
│                    │          │     │                      │
│   Page content     │          │     │                      │
│                    │          │     │                      │
├────────────────────┤          │     │                      │
│  🏠  Library  ⚙️   │          └─────┴──────────────────────┘
└────────────────────┘
```

- **Mobile:** top bar (title + search icon) + fixed bottom tab bar (Library, Settings).
  Safe-area insets applied via `env(safe-area-inset-bottom)` to avoid the iOS home
  indicator.
- **md+:** persistent left sidebar with nav links; no bottom bar. Sidebar collapses to
  icon-only at `lg` if needed to save space.
- **4K:** page content wrapped in `max-w-screen-2xl mx-auto` — prevents line lengths
  from becoming uncomfortable on very wide displays.

### Document Library (`/`)

```
Mobile                   md (tablet)              lg+ (desktop)
┌──────────────┐         ┌────────┬──────────┐    ┌──────┬──────────────┬─────────┐
│ [Search bar] │         │ Filter │ Doc list │    │Filter│  Doc list    │ Details │
│ [Filter btn] │         │ panel  │          │    │panel │              │ sidebar │
│ Doc list     │         │        │ [Details │    │      │              │         │
│ (full width) │         │        │  drawer] │    │      │              │         │
└──────────────┘         └────────┴──────────┘    └──────┴──────────────┴─────────┘
```

- **Mobile:** single-column list, full-width rows. Tag filter hidden behind a "Filter"
  button that opens a bottom sheet. Tapping a document navigates to a dedicated
  `/documents/[fingerprint]` details page — no inline sidebar.
- **md:** list + slide-in details panel (overlay, 70% width). Filter panel collapsible
  above the list.
- **lg+:** three-column layout — fixed-width filter panel on the left, scrollable
  document list in the centre, resizable details sidebar on the right.
- **2xl+:** overall layout capped at `max-w-screen-2xl`; columns use proportional
  `flex` widths rather than fixed pixel values to fill large screens gracefully.

Document list rows show fewer columns on narrow viewports:

| Column | Mobile | sm | md+ |
|--------|--------|-----|-----|
| Name | ✓ | ✓ | ✓ |
| Type badge | ✓ | ✓ | ✓ |
| Tags | — | ✓ | ✓ |
| Size | — | — | ✓ |
| Directory | — | — | ✓ |

### PDF and EPUB Viewers

Both viewers take over the full viewport — there is no surrounding shell chrome while
reading.

```
Mobile                              Desktop / 4K
┌──────────────────────────────┐    ┌────────────────────────────────────────────┐
│ ← Back    Ch 3 / 12    ···   │    │ ← Back   Title          Font ± / Theme  ✕ │
│ ─────────────────────────── │    ├────────────────────────────────────────────┤
│                              │    │                                            │
│      Book / PDF content      │    │         Book / PDF content                 │
│      (full screen)           │    │         (centred, max reading width)       │
│                              │    │                                            │
│ ─────────────────────────── │    ├────────────────────────────────────────────┤
│ ◀◀  Page 4 / 230  ▶▶        │    │ ◀  Page 4 / 230  ▶              Zoom 100% │
└──────────────────────────────┘    └────────────────────────────────────────────┘
```

- **Mobile:** toolbar auto-hides after 3 s of inactivity; tap the centre of the screen
  to toggle it. Swipe left/right to turn EPUB pages. Pinch-to-zoom on PDF pages.
  Controls (prev/next, back) are at least 44×44 px (WCAG 2.5.5 / Apple HIG minimum).
- **Tablet:** toolbar always visible at top and bottom. Side-tap zones (left/right 20%
  of viewport) turn pages, reducing the need to reach the toolbar buttons.
- **Desktop/4K:** content centred in a column with a configurable max reading width
  (default ~70 ch for EPUB, full page width for PDF). Controls always visible. Keyboard
  navigation: arrow keys / space to page, Escape to return to library.

### Forms (Source Management, Settings)

- **Mobile:** single-column stacked fields, full-width inputs. `font-size: 16px` on all
  inputs to prevent iOS Safari from zooming on focus.
- **md+:** form constrained to `max-w-lg`, labels beside inputs where space allows.

### Touch Targets

All interactive elements must meet a 44×44 px minimum tap target on mobile, achieved
with Tailwind padding utilities (`p-3`, `py-3 px-4`, etc.) rather than relying on the
visible element size alone.

### Typography Scaling

- Base font size: `16px` across all viewports (prevents iOS zoom on focused inputs).
- Reading views: user-adjustable font size (stored in preferences), default `18px`,
  range `14px`–`28px`.
- At `2xl`, UI chrome text scales with Tailwind's `text-base` / `text-lg` utilities —
  no separate type scale is needed.

---

## SvelteKit Project Layout

```
pwa/
├── src/
│   ├── app.html                    # HTML shell (PWA meta tags, manifest link)
│   ├── lib/
│   │   ├── api/
│   │   │   ├── client.ts           # Typed HTTP client (Basic Auth, all endpoints)
│   │   │   └── aggregator.ts       # Multi-source fan-out & merge logic
│   │   ├── db/
│   │   │   └── index.ts            # Dexie schema + typed table exports
│   │   ├── stores/
│   │   │   ├── sources.ts          # Reactive Svelte store backed by IndexedDB
│   │   │   └── documents.ts        # Merged + filtered document list
│   │   └── components/
│   │       ├── TagFilter.svelte    # Allow/deny tag filter panel
│   │       ├── TagEditor.svelte    # Add/remove tags with autocomplete
│   │       ├── SourceForm.svelte   # Add/edit remote source form
│   │       └── DocumentRow.svelte  # Single row in the document list
│   └── routes/
│       ├── +layout.svelte          # Top nav, source status indicators
│       ├── +page.svelte            # Document list (/)
│       ├── settings/
│       │   ├── +page.svelte        # General app settings
│       │   └── sources/
│       │       └── +page.svelte    # Source management
│       └── read/
│           ├── epub/[fingerprint]/
│           │   └── +page.svelte    # EPUB reader (epub.js)
│           └── pdf/[fingerprint]/
│               └── +page.svelte    # PDF reader (pdf.js)
├── static/
│   ├── icons/                      # PWA icons (192.png, 512.png)
│   └── favicon.png
├── package.json
├── svelte.config.js                # adapter-static; SPA fallback (fallback: 'index.html')
├── vite.config.ts                  # vite-plugin-pwa manifest + workbox config
└── tsconfig.json
```

---

## API Client Interface

Wraps the read-flow REST API. Every method is scoped to a configured `Source`:

```typescript
interface ReadFlowClient {
  status(): Promise<Status>
  getFiles(): Promise<File[]>
  getFile(guid: string): Promise<File>
  getAllTags(): Promise<string[]>
  addTags(guid: string, tags: string[]): Promise<string[]>
  deleteTags(guid: string, tags: string[]): Promise<string[]>
  getReadingProgress(fingerprint: string): Promise<ReadingProgress | null>
  upsertReadingProgress(progress: ReadingProgress): Promise<void>
  downloadFile(guid: string, fileName: string): Promise<Blob>
}
```

Auth header: `Authorization: Basic <base64(userId:passphrase)>`

The server already sets `Access-Control-Allow-Origin: *`, so cross-origin requests
work without a proxy.

---

## Implementation Order

1. Scaffold — `npm create svelte@latest pwa`, configure `adapter-static`, Tailwind, vite-plugin-pwa
2. Dexie schema + typed tables
3. Source management — IndexedDB store + `/settings/sources` page + `status()` connectivity check
4. Document list — `getFiles()` aggregator, fuzzy search, tag filter, details sidebar
5. Tag management — add/remove with fan-out writes
6. PDF viewer — pdf.js integration, progress tracking
7. EPUB viewer — epub.js integration, CFI progress tracking
8. Reading progress sync — cross-source fetch + write on open/update
9. PWA polish — manifest icons, service worker cache tuning, install prompt

---

## Verification Checklist

- `npm run build` produces a static site in `build/`; no TypeScript errors (`npm run check`)
- `npm run preview` serves the build — Chrome "Install" button appears in the address bar
- Add a local `read-flow` server as a source; document list loads correctly
- Open a PDF — renders, page navigation works, closing and reopening resumes at last page
- Open an EPUB — renders, navigation works, closing and reopening resumes at CFI location
- Add/remove a tag — change reflected on the next document list refresh
- Configure two sources pointing at the same server; document list deduplicates by fingerprint
- Reading progress set in the PWA is visible in the cosmic desktop app (and vice versa)
