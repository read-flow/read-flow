# EPUB Subsystem

EPUB3 parsing and rendering pipeline for Read Flow — a backend integrated into the
unified reading runtime, not a standalone reader.

**Design goals:** renderer-independent reading position · deterministic cross-device
sync · format-agnostic document model · stable structural addressing · replaceable
rendering engines · native COSMIC integration.

## Crate layout

```
epub/src/
├── lib.rs
├── error.rs
├── content/
│   ├── block.rs          # ContentBlock, TextSpan, BlockStyle, ListItem, TableCell
│   ├── parser/           # html5ever → Vec<ContentBlock> (classify, start/end tags, state)
│   ├── resolve.rs        # href/path resolution
│   └── stylesheet.rs     # StyleSheet, CssSelector, parse_css()
├── domain/
│   ├── document.rs       # Document trait
│   ├── locator.rs        # Locator with rf:// URI round-trip
│   ├── metadata.rs       # DocumentMetadata
│   ├── nav.rs            # Navigation model
│   └── spine.rs          # SpineItem
├── epub/
│   ├── container.rs      # META-INF/container.xml
│   ├── nav.rs            # EPUB3 nav.xhtml / EPUB2 NCX
│   ├── package.rs        # OPF manifest/spine/metadata
│   └── document.rs       # EpubDocument
└── renderer/
    ├── traits.rs          # Renderer, RenderSurface
    ├── types.rs           # RendererCapabilities, RenderFrame
    └── protocol.rs        # RendererCommand, RendererEvent
```

The COSMIC viewer lives in `cosmic/src/page/epub_viewer/`.

**Key content types** (`src/content/block.rs`):

| Type           | Description                                                                                                                                 |
|----------------|---------------------------------------------------------------------------------------------------------------------------------------------|
| `ContentBlock` | Tagged union: Paragraph, Heading, Preformatted, BlockQuote, UnorderedList, OrderedList, Image, Svg, Table, Figure, HorizontalRule, Footnote, Anchor |
| `TextSpan`     | Rich text run: `text`, `InlineStyle` (bold/italic/underline/strikethrough/monospaced), optional `link`/`color`/`font_size_em`               |
| `BlockStyle`   | Block-level CSS: `text_align`, `font_size_em`, `color`, `margin_top_em`, `margin_bottom_em`                                                 |
| `ListItem`     | `text` + `spans` + `style`                                                                                                                  |
| `TableCell`    | `text` + `spans` + `is_header`                                                                                                              |

## Reading position model

**Locator format:** `rf://doc/<content_hash>/spine/<index>/node/<path>/char/<offset>`

**ReadingProgress** (generic opaque JSON stored by the server):

```json
{"chapter": 2, "scroll": 340.5, "block": 15, "mode": "paginated"}
```

| Field     | Type   | Purpose                                       |
|-----------|--------|-----------------------------------------------|
| `chapter` | usize  | Active spine/chapter index                    |
| `scroll`  | f32    | Scroll-mode y-offset                          |
| `block`   | usize  | First visible block index (paginated restore) |
| `mode`    | string | `"scroll"` or `"paginated"`                   |

All fields are optional for backward compatibility. `block` survives re-pagination
across viewport sizes.

## Paginated rendering (COSMIC viewer)

Pagination is a **view-layer concern**. The document model (`ContentBlock`,
`SpineItem`) is unchanged; the viewer splits blocks into pages at render time based
on the current viewport dimensions.

Core types (`cosmic/src/page/epub_viewer/`):

```rust
enum ViewMode { Scroll, Paginated }

struct PageRange {
    start: usize,             // [start..end) block indices
    start_char_offset: usize, // > 0: first block continues a split paragraph
    end: usize,
    end_char_offset: usize,   // > 0: last block continues on the next page
}

struct PaginationLayout { page_height: f32, page_width: f32, pages: Vec<PageRange> }

enum DualPageMode { Auto, Off, On }
```

**Algorithm** (`paginate_blocks`): walk blocks accumulating measured heights plus
inter-block spacing; close a page when the accumulated height exceeds `page_height`.
Text heights are measured with `cosmic-text` shaping (`measure_text_height`), so
pagination is pixel-accurate rather than heuristic. Paragraphs taller than the
remaining space are split mid-block at a character offset (`start_char_offset` /
`end_char_offset`, spans sliced with metadata preserved). Images and SVGs use their
natural dimensions (decoded at load / parsed from `viewBox`), scaled to content
width and capped to the page height.

**Cache and invalidation:** `pagination_cache: HashMap<usize, PaginationLayout>`,
one layout per visited chapter. `maybe_repaginate()` runs every `update()`: it
compares the cached dimensions against the current viewport (1 px tolerance,
reported by `widget::responsive`) and recomputes when they differ, clamping
`current_page` afterwards. The cache is cleared on dual-page toggle and on font
size/family changes.

**Rendering:** only the blocks of the current page (or two pages in dual-page mode)
are rendered. Click-to-turn zones: 10% left / 80% center / 10% right.

## Viewer UX

Context panel controls:

| Control         | Field                  | Range                       | Effect                                     |
|-----------------|------------------------|-----------------------------|--------------------------------------------|
| Paginated view  | `view_mode`            | Scroll / Paginated          | Continuous scroll vs page-flipping         |
| Two-page spread | `dual_page`            | Off / Auto / On             | Side-by-side; Auto at viewport > 1200 px   |
| Page fill       | `page_height_fraction` | 50–100%                     | Fraction of viewport used for page content |
| Navigation pane | `show_sidebar`         | on/off                      | Chapter list sidebar                       |
| Content width   | `content_width_pct`    | 50–150%                     | Max-width of text column (base 800 px)     |
| Font            | `font_family`          | Serif / Sans / Mono / named | Persisted via cosmic-config                |
| Font size       | `base_font_size`       | 12–24 px                    | Scales all text proportionally; persisted  |
| Show raw HTML   | `show_raw_html`        | on/off                      | Debug: renders chapter as monotext         |

**Search:** Ctrl+F toggles a search bar. `block_contains()` does recursive
case-insensitive substring match across all `ContentBlock` variants.
`BlockHighlight::SearchMatch` (20% accent tint) marks all matches;
`BlockHighlight::Current` (full accent) marks the focused one. Prev/next and Enter
cycle; Escape dismisses; cleared on chapter navigation.

**Font persistence:** `load_epub_font_prefs()` / `save_epub_font_prefs()` use
`ConfigGet` / `ConfigSet` with app ID `com.github.read-flow.read-flow`, version 1.
Keys: `epub_font_family` (String), `epub_base_font_size` (u32 pixels). Named fonts
round-trip via a `fonts()` list lookup.

## Future directions

- Persist additional viewer settings via cosmic-config: `view_mode`,
  `show_sidebar`, `content_width_pct`, `dual_page`, `page_height_fraction`
- Distributed reading runtime — multi-device progress merge using a structural
  distance metric (`spine_delta * LARGE_WEIGHT + node_path_diff + char_offset_delta`)
- Annotation system — highlight ranges, notes, cross-device sync
- Headless structural renderer — background locator resolution and progress
  computation without a display
- Error propagation in renderer traits — `Result` return types for
  `load_document`, `load_spine_item`, `go_to` once error patterns stabilize
