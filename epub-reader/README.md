# EPUB Reader for COSMIC Desktop

Open a local EPUB from COSMIC Files or launch **EPUB Reader** and choose
**Open File**. Each book opens in its own window. Positions are saved locally
per file as you turn pages and follow the table of contents.

Use the page arrows, Left and Right, or Page Up and Page Down to turn pages.
The header bar has text size controls and a choice of **Single**,
**Two pages**, and **Automatic** page layouts.

## Local installation

Run from the workspace root on a COSMIC Desktop system.

```sh
cargo build --locked --release -p cosmic-epub-reader
sh epub-reader/install-local.sh
```

The installer puts the binary in `$XDG_BIN_HOME` or `~/.local/bin`. It
installs the launcher and icon under `$XDG_DATA_HOME` or `~/.local/share`.
Choose **Open With → EPUB Reader** for an EPUB in COSMIC Files.
