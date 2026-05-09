# Read Flow PWA

A Progressive Web App for browsing and reading documents from remote
[read-flow](../core/) servers. Built with SvelteKit, TypeScript, and Tailwind CSS.

## Prerequisites

- Node.js 20 or newer
- npm 10 or newer

## Getting started

```bash
cd pwa
npm install
npm run dev
```

The development server starts at `http://localhost:5173`.

> **Note:** The service worker (PWA offline support) is only active in the production
> build. To test PWA features, use `npm run preview` after building.

## Available commands

| Command | Description |
|---------|-------------|
| `npm run dev` | Start the Vite development server with hot-module reload |
| `npm run build` | Build for production (outputs to `build/`) |
| `npm run preview` | Preview the production build locally |
| `npm run check` | Run `svelte-check` (TypeScript + Svelte type checking) |
| `npm run check:watch` | Same as above, in watch mode |

## Connecting to a read-flow server

1. Open the app and navigate to **Settings → Sources**.
2. Click **Add source** and enter:
   - **Name** — a friendly label (e.g. "Home server")
   - **Base URL** — the full URL of the server (e.g. `http://192.168.1.10:8000`)
   - **User ID** and **Passphrase** — as configured in `archive-organizer.toml`
3. The app tests connectivity before saving. Once added, the Library page
   fetches documents from all configured sources.

Multiple sources are supported. Documents that appear on more than one source are
deduplicated by content fingerprint. Tag changes and reading progress are written to
all sources in parallel.

## PWA installation

After running `npm run build && npm run preview`, open `http://localhost:4173` in
Chrome or Edge. An install button appears in the address bar. On mobile, use
**Add to Home Screen** from the browser menu.

## Project structure

```
pwa/
├── src/
│   ├── app.html              # HTML shell (viewport, PWA meta tags)
│   ├── app.css               # Global styles (Tailwind v4 import)
│   ├── lib/
│   │   ├── api/
│   │   │   ├── client.ts     # Typed HTTP client for one read-flow server
│   │   │   └── aggregator.ts # Multi-source fan-out and merge logic
│   │   ├── db/
│   │   │   └── index.ts      # Dexie (IndexedDB) schema and typed tables
│   │   ├── stores/
│   │   │   ├── sources.ts    # Svelte store for configured sources
│   │   │   └── documents.ts  # Merged document list with search/filter
│   │   └── components/
│   │       └── Icon.svelte   # Inline SVG icon set
│   └── routes/
│       ├── +layout.ts        # Disables SSR/prerendering (SPA mode)
│       ├── +layout.svelte    # App shell: sidebar (desktop) / bottom nav (mobile)
│       ├── +page.svelte      # Library — document list with search and tag filter
│       ├── documents/[fingerprint]/
│       │   └── +page.svelte  # Document details (navigated to on mobile)
│       ├── settings/
│       │   ├── +page.svelte         # Settings index
│       │   └── sources/+page.svelte # Source management
│       └── read/
│           ├── +layout.svelte               # Full-screen reader shell (no nav)
│           ├── epub/[fingerprint]/+page.svelte  # EPUB reader (epub.js)
│           └── pdf/[fingerprint]/+page.svelte   # PDF reader (pdfjs-dist)
├── static/
│   └── icons/               # PWA icons (192×192 and 512×512 PNG, not yet included)
├── package.json
├── svelte.config.js         # adapter-static with SPA fallback
├── vite.config.ts           # Tailwind v4, vite-plugin-pwa
└── tsconfig.json
```

## Local storage

All user data is stored in the browser's IndexedDB database (`ReadFlowDB`) — never
on a server. Three tables are used:

| Table | Contents |
|-------|---------|
| `sources` | Configured remote servers (URL, credentials, order) |
| `readingProgress` | Local cache of reading position by file fingerprint |
| `preferences` | UI preferences (pane widths, theme, etc.) |

Credentials are stored in IndexedDB (origin-sandboxed). They are **not** encrypted
at rest; the same risk model as browser-saved passwords applies.

## Reading progress sync

Progress is keyed by `fingerprint` (SHA-256 of the file content), which is identical
for the same document on different sources and in the COSMIC desktop app. When you
open a document the app fetches progress from all sources and uses the most recently
updated entry. Updates are written back to all sources in parallel.

## Design document

See [`DESIGN.md`](./DESIGN.md) for architecture decisions, library assessments, and
the full feature roadmap.
