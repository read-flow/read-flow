# Design: PDF Viewer Integration

## Goal

Integrate the `cosmic-reader` PDF viewer into the `archive-organizer-cosmic` application as a new page type, allowing users to view PDF documents directly from the document details page. The viewer must fit within the existing Pages framework, with thumbnails rendered inline (not in the nav-bar).

## Decisions

- **cosmic-reader is superseded** by this integration. The PDF rendering logic is extracted directly into the `cosmic` crate. No shared crate is needed.
- **mupdf backend only** — the `lopdf` alternative is not carried over. The mupdf feature is added as a direct (non-optional) dependency since this is the primary use case.
- **Single-page view** initially — one page at a time with thumbnail navigation. Continuous scroll can be added later.
- **Local files first** — the viewer resolves local sources via `Document::sources_by_priority()`. Remote-only documents are not supported initially (the existing "Open Document" button already handles download-then-open via xdg-open).

## Current State

### cosmic-reader

- Standalone COSMIC application using the `mupdf` backend
- Uses `nav_bar::Model` to store `Page` data and manage page selection
- Thumbnails rendered in `nav_bar()` as a scrollable column of image buttons (128px wide)
- Main view renders the active page as SVG with zoom/pan support
- Async pipeline: PDF load -> `DisplayList` per page -> thumbnail PNG + page SVG
- Supports zoom (fit-width/height/both, percentage), search, keyboard navigation

### archive-organizer-cosmic

- Pages framework: `PageSelector` enum routes to page structs via `Pages::view()`/`Pages::update()`
- Dynamic pages supported (e.g., `DocumentDetails` keyed by `Fingerprint`)
- Nav-bar managed by `AppModel`, with `PageSelector` stored as data on each nav entity
- State management via `ProvidedState<P, T>` and `DocumentProvider`

## Design

### New Page: `PdfViewer`

Add a new dynamic page `PdfViewer` to the Pages framework, similar to how `DocumentDetails` works.

#### PageSelector

```rust
pub enum PageSelector {
    Sources,
    Documents,
    DocumentDetails(Fingerprint),
    PdfViewer(Fingerprint),  // new
    Settings,
}
```

#### Page struct

```rust
// cosmic/src/page/pdf_viewer.rs

pub struct PdfViewer {
    fingerprint: Fingerprint,
    document: Document,

    // PDF state (extracted from cosmic-reader App)
    pages: Vec<PdfPage>,
    active_page: usize,
    zoom: Zoom,
    zoom_names: Vec<String>,
    search_active: bool,
    search_id: widget::Id,
    search_term: String,
    modifiers: Modifiers,
    view_ratio: Cell<f32>,
    zoom_scroll: f32,

    // Thumbnail panel state
    thumbnail_scroll_id: widget::Id,
    thumbnail_viewport: Option<scrollable::Viewport>,
}
```

#### PdfPage (renamed from cosmic-reader's `Page`)

```rust
struct PdfPage {
    index: i32,
    bounds: mupdf::Rect,
    display_list: Option<Arc<mupdf::DisplayList>>,
    icon_bounds: Cell<Option<Rectangle>>,
    icon_handle: Option<widget::image::Handle>,   // thumbnail
    svg_handle: Option<widget::svg::Handle>,      // main render
}
```

### Layout: Thumbnails as Inline Panel

Instead of using the COSMIC `nav_bar()` override, thumbnails are rendered as part of `PdfViewer::view()`:

```
+-----------------------------------------------------------+
|  [< Back]   filename.pdf   [Zoom controls] [Search]      |
+----------+------------------------------------------------+
|          |                                                |
| thumb 1  |                                                |
| [=====]  |           Main PDF Page View                   |
|          |           (SVG, zoomable, pannable)             |
| thumb 2  |                                                |
| [     ]  |                                                |
|          |                                                |
| thumb 3  |                                                |
| [     ]  |                                                |
|          |                                                |
| ...      |                                                |
|          |                                                |
+----------+------------------------------------------------+
```

The `view()` method composes:

1. **Header row** — back button, document title, zoom dropdown, search toggle
2. **Body row** — two columns:
   - **Left**: scrollable thumbnail column (fixed width ~128px), with active page highlighted
   - **Right**: main page content (responsive SVG with zoom/pan), wrapped in a scrollable container

```rust
impl PdfViewer {
    pub fn view(&self) -> Element<PdfViewerMessage> {
        let header = self.view_header();
        let thumbnails = self.view_thumbnails();  // scrollable column
        let content = self.view_content();        // responsive SVG

        let body = widget::row()
            .push(thumbnails)
            .push(content);

        widget::column()
            .push(header)
            .push(body)
            .into()
    }
}
```

### Message Flow

```rust
pub enum PdfViewerMessage {
    // PDF loading pipeline
    PagesLoaded(Vec<PdfPage>),
    DisplayListReady(i32, Arc<mupdf::DisplayList>),
    ThumbnailReady(i32, widget::image::Handle),
    SvgReady(i32, widget::svg::Handle),

    // Navigation
    SelectPage(usize),
    NextPage,
    PreviousPage,
    ThumbnailScroll(scrollable::Viewport),

    // Zoom
    ZoomDropdown(usize),
    ZoomScroll(ScrollDelta),

    // Search
    SearchActivate,
    SearchClear,
    SearchInput(String),

    // Keyboard / input
    Key(Modifiers, Key, Option<SmolStr>),
    ModifiersChanged(Modifiers),

    // Outgoing
    Out(PdfViewerOutput),
}

pub enum PdfViewerOutput {
    Close(Fingerprint),
}
```

### Opening the PDF Viewer

The PDF viewer is launched from `DocumentDetails` when the user clicks the existing "Open Document" icon button (for PDF documents, this will now open the in-app viewer instead of xdg-open).

1. `DocumentDetails` emits `DocumentDetailsOutput::ViewPdf(document)` (for PDF type only)
2. `Pages::update()` maps this to `PageMessage::OpenPdfViewer(document)`
3. `Pages` creates a new `PdfViewer` instance, stores it in `IndexMap<Fingerprint, PdfViewer>`
4. Emits `PageOutput::PageAdded(PageSelector::PdfViewer(fingerprint), "application-pdf-symbolic")`
5. `AppModel` adds a nav entry and activates it

Closing follows the same pattern as `DocumentDetails` — navigate back to parent via `PageOutput::PageRemoved`.

### PDF Loading Pipeline

The loading is done via `Task::perform` + `tokio::task::spawn_blocking`, matching the cosmic-reader approach but using Tasks instead of Subscriptions:

1. On `PdfViewer::new()`, resolve the local file path from `document.sources_by_priority()`
2. Spawn a blocking task that opens the mupdf document, reads page count and bounds, returns `Vec<PdfPage>`
3. On `PagesLoaded`, store pages and spawn display list generation tasks for each page
4. On `DisplayListReady(index, display_list)`, store the display list and spawn thumbnail generation
5. On `ThumbnailReady(index, handle)`, store the thumbnail image handle
6. SVG rendering is on-demand: triggered by `update_active_page()` only for the currently selected page

### File Resolution

Documents in the archive have sources (local file paths or remote URLs). To open a PDF:

1. Use `document.sources_by_priority()` which returns local sources first
2. Find the first local source and use its `path` field directly as a `PathBuf`
3. Pass to the mupdf loading pipeline

If no local source exists, fall back to the existing `xdg_open_file` behavior.

### Keyboard Shortcuts

Reuse cosmic-reader's keyboard handling within the `PdfViewer` page:

| Key | Action |
|-----|--------|
| Arrow Up/Left, PageUp | Previous page |
| Arrow Down/Right, PageDown | Next page |
| `0` | Zoom 100% |
| `-` / `=` | Zoom out/in (25% steps) |
| `f` | Fit both |
| `h` | Fit height |
| `w` | Fit width |
| `s` / `/` | Activate search |
| Escape | Close search |
| Ctrl+Scroll | Zoom with mouse wheel |

These only apply when the `PdfViewer` page is active. Keyboard events are delivered via the COSMIC subscription system and forwarded to the active page.

## Implementation Steps

### Phase 1: Dependencies and PDF Core Types

1. Add `mupdf` dependency to `cosmic/Cargo.toml`
2. Create `cosmic/src/page/pdf_viewer.rs` with core types: `PdfPage`, `Zoom`, helper functions (`display_list_to_image`)

### Phase 2: PdfViewer Page Implementation

3. Implement `PdfViewer` struct with `new()`, `update()`, `view()`
4. Implement the inline thumbnail panel (`view_thumbnails()`)
5. Implement the main content area with zoom/pan (`view_content()`)
6. Implement header with zoom dropdown and search (`view_header()`)
7. Wire up the async loading pipeline (pages -> display lists -> thumbnails + SVGs)

### Phase 3: Pages Framework Integration

8. Add `PdfViewer(Fingerprint)` to `PageSelector`
9. Add `PdfViewerMessage` to `PageMessage` with `From` impl
10. Add `pdf_viewers: IndexMap<Fingerprint, PdfViewer>` to `Pages` struct
11. Update `Pages::view()`, `Pages::update()`, `Pages::display_name()`, `Pages::view_context()`
12. Add `OpenPdfViewer(Document)` / `ClosePdfViewer(Fingerprint)` messages to `PageMessage`
13. Modify `DocumentDetails::OpenDocument` to open the in-app PDF viewer for PDF documents

### Phase 4: Polish

14. Add keyboard shortcuts (scoped to PdfViewer page)
15. Add loading states (spinner while PDF loads, placeholder thumbnails)
16. Test with various PDF files

## Open Questions

- **Search UI**: Should search results be shown in-page (highlights) or in a list? cosmic-reader had a TODO for search result rendering. Defer to Phase 4 or later.
- **Continuous scroll**: Should the viewer support continuous vertical scrolling through all pages? Start with single-page view, consider adding later.

---

## Work In Progress

### Phase 1: Dependencies and PDF Core Types
- [x] Add `mupdf` dependency to `cosmic/Cargo.toml`
- [x] Create `cosmic/src/page/pdf_viewer.rs` with `PdfPage`, `Zoom`, `display_list_to_image()`

### Phase 2: PdfViewer Page Implementation
- [x] `PdfViewer` struct, `PdfViewerMessage`, `PdfViewerOutput`
- [x] `PdfViewer::new()` — resolve file path, start loading
- [x] `PdfViewer::update()` — handle all messages
- [x] `PdfViewer::view_header()` — back button, title, zoom, search
- [x] `PdfViewer::view_thumbnails()` — scrollable thumbnail column
- [x] `PdfViewer::view_content()` — main SVG page view with zoom/pan
- [x] Async loading pipeline (display lists, thumbnails, SVGs)

### Phase 3: Pages Framework Integration
- [x] `PageSelector::PdfViewer(Fingerprint)`
- [x] `PageMessage::PdfViewer(Fingerprint, PdfViewerMessage)` + mapping
- [x] `Pages` struct: `pdf_viewers: IndexMap<Fingerprint, PdfViewer>`
- [x] `Pages::view/update/display_name/view_context` updates
- [x] `OpenPdfViewer`/`ClosePdfViewer` messages
- [x] Modify `DocumentDetails` to open PDF viewer for PDF documents
- [x] Add `DocumentDetailsOutput::ViewPdf` for PDF-specific open action
- [x] Add `DocumentType: PartialEq + Eq` derive
- [x] Add i18n strings (`pdf-viewer-back`, `pdf-viewer-loading`, `pdf-viewer-view-pdf`)

### Phase 4: Polish
- [ ] Keyboard shortcuts (Key/ModifiersChanged variants exist but not yet wired to subscriptions)
- [ ] Loading states / spinners
- [ ] Testing
