#!/bin/sh
set -eu

reader_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
binary=${1:-"$reader_dir/../target/release/cosmic-epub-reader"}
bin_dir=${XDG_BIN_HOME:-"$HOME/.local/bin"}
data_dir=${XDG_DATA_HOME:-"$HOME/.local/share"}
app_id=io.github.read_flow.CosmicEpubReader
launcher="$data_dir/applications/$app_id.desktop"
icon_dir="$data_dir/icons/hicolor/scalable/apps"

test -x "$binary" || {
    printf 'Build cosmic-epub-reader first, or pass the path to the built binary.\n' >&2
    exit 1
}

case "$bin_dir" in
    /*) ;;
    *) printf 'XDG_BIN_HOME must be an absolute path.\n' >&2; exit 1 ;;
esac

mkdir -p "$bin_dir" "$data_dir/applications" "$icon_dir"
install -m 755 "$binary" "$bin_dir/cosmic-epub-reader"
install -m 644 "$reader_dir/resources/$app_id.svg" "$icon_dir/$app_id.svg"

# A desktop session may omit the user bin directory from PATH. Quote the
# absolute executable path for the Exec parser, then escape the key value.
escaped_exec=$(printf '%s' "$bin_dir/cosmic-epub-reader" |
    sed 's/[%]/%%/g; s/[\\"`$]/\\&/g; s/\\/\\\\/g')
while IFS= read -r line; do
    case "$line" in
        Exec=*) printf 'Exec="%s" %%f\n' "$escaped_exec" ;;
        *) printf '%s\n' "$line" ;;
    esac
done < "$reader_dir/resources/$app_id.desktop" > "$launcher"

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$data_dir/applications"
fi
