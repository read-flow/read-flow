# EPUB Reader for COSMIC Desktop

Open a local EPUB from COSMIC Files or launch **EPUB Reader** and choose
**File → Open File…** (Ctrl+O). Each book opens in its own window. Positions are
saved locally per file as you turn pages and follow the table of contents.

Use the page arrows, Left and Right, or Page Up and Page Down to turn pages.
The header bar has text size controls (−/+) and a choice of **Single**,
**Two pages**, and **Automatic** page layouts.

### Contents

Books with a table of contents show a contents sidebar. Use the toggle button in
the header bar, or **View → Contents**, to show or hide it. Select an entry to
jump to it. Entries with sub-sections can be expanded and collapsed. In a narrow
window the contents take the place of the page, and you return to the book by
selecting an entry or toggling the sidebar again.

### About

**View → About** shows the version, license, and a link to the project
repository.

### Language

The interface is available in English, French, and Dutch. It follows your
system language; there is no separate language setting in the reader.

### Troubleshooting

Warnings are written to standard error as JSON lines. Set `RUST_LOG` (for
example `RUST_LOG=debug`) to change how much is logged.

## Local installation

Run from the workspace root on a COSMIC Desktop system.

```sh
cargo build --locked --release -p cosmic-epub-reader
sh epub-reader/install-local.sh
```

The installer puts the binary in `$XDG_BIN_HOME` or `~/.local/bin`. It
installs the launcher and icon under `$XDG_DATA_HOME` or `~/.local/share`.
Choose **Open With → EPUB Reader** for an EPUB in COSMIC Files.
