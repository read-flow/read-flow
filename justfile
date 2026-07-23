name := 'read-flow'
appid := 'io.github.read-flow'

rootdir := ''
prefix := '/usr'

base-dir := absolute_path(clean(rootdir / prefix))

bin-src := 'target' / 'release' / name
bin-dst := base-dir / 'bin' / name

desktop := appid + '.desktop'
desktop-src := 'cosmic' / 'resources' / desktop
desktop-dst := clean(rootdir / prefix) / 'share' / 'applications' / desktop

appdata := appid + '.metainfo.xml'
appdata-src := 'cosmic' / 'resources' / appdata
appdata-dst := clean(rootdir / prefix) / 'share' / 'appdata' / appdata

icons-src := 'cosmic' / 'resources' / 'icons' / 'hicolor'
icons-dst := clean(rootdir / prefix) / 'share' / 'icons' / 'hicolor'

icon-svg-src := icons-src / 'scalable' / 'apps' / 'icon.svg'
icon-svg-dst := icons-dst / 'scalable' / 'apps' / appid + '.svg'

app-name := 'Read Flow'
app-bundle := 'target' / 'release' / app-name + '.app'
iconset := 'target' / 'ReadFlow.iconset'
icns := 'target' / 'ReadFlow.icns'

# Default recipe which runs `just build-release`
default: build-release

# Runs `cargo clean`
clean:
    cargo clean

# Removes vendored dependencies
clean-vendor:
    rm -rf .cargo vendor vendor.tar

# `cargo clean` and removes vendored dependencies
clean-dist: clean clean-vendor

# Compiles with debug profile
build-debug *args:
    cargo build {{args}}

# Compiles with release profile
build-release *args: (build-debug '--release' args)

# Compiles release profile with vendored dependencies
build-vendored *args: vendor-extract (build-release '--frozen --offline' args)

# Runs workspace tests
test *args:
    cargo nextest run {{args}}

# Runs the cucumber-rs BDD harness (BDD_DRIVER=rest|cosmic, default rest)
bdd driver='rest':
    BDD_DRIVER={{driver}} cargo nextest run -p read-flow bdd

# Runs a clippy check
check *args:
    cargo clippy --all-features {{args}} -- -W clippy::pedantic

# Runs a clippy check with JSON message format
check-json: (check '--message-format=json')

# Run the COSMIC desktop app for testing purposes
run *args:
    env RUST_BACKTRACE=full cargo run -p read-flow --release {{args}}

# Run the desktop app with the embedded PWA so its in-app server hosts the web UI
run-embedded *args: pwa-build
    env RUST_BACKTRACE=full cargo run -p read-flow --release --features embed-pwa {{args}}

# Run the headless server with the embedded PWA (builds the PWA first)
serve *args: pwa-build
    cargo run -p read-flow --release --features embed-pwa -- --headless {{args}}

# Installs files
install:
    install -Dm0755 {{bin-src}} {{bin-dst}}
    install -Dm0644 {{desktop-src}} {{desktop-dst}}
    install -Dm0644 {{appdata-src}} {{appdata-dst}}
    install -Dm0644 {{icon-svg-src}} {{icon-svg-dst}}

# Uninstalls installed files
uninstall:
    rm {{bin-dst}} {{desktop-dst}} {{icon-svg-dst}}

# Builds a .deb package for the COSMIC desktop app (with embedded PWA)
deb: pwa-build (build-release '--features' 'embed-pwa' '-p' 'read-flow')
    cargo deb -p read-flow --no-build

# Vendor dependencies locally
vendor:
    #!/usr/bin/env bash
    mkdir -p .cargo
    cargo vendor --sync Cargo.toml | head -n -1 > .cargo/config.toml
    echo 'directory = "vendor"' >> .cargo/config.toml
    echo >> .cargo/config.toml
    echo '[env]' >> .cargo/config.toml
    if [ -n "${SOURCE_DATE_EPOCH}" ]
    then
        source_date="$(date -d "@${SOURCE_DATE_EPOCH}" "+%Y-%m-%d")"
        echo "VERGEN_GIT_COMMIT_DATE = \"${source_date}\"" >> .cargo/config.toml
    fi
    if [ -n "${SOURCE_GIT_HASH}" ]
    then
        echo "VERGEN_GIT_SHA = \"${SOURCE_GIT_HASH}\"" >> .cargo/config.toml
    fi
    tar pcf vendor.tar .cargo vendor
    rm -rf .cargo vendor

# Extracts vendored dependencies
vendor-extract:
    rm -rf vendor
    tar pxf vendor.tar

# ── macOS ─────────────────────────────────────────────────────────────────────

# Generate macOS .icns icon from SVG (requires rsvg-convert: brew install librsvg)
[macos]
icon:
    #!/usr/bin/env bash
    set -euo pipefail
    mkdir -p "{{iconset}}"
    for size in 16 32 128 256 512; do
        rsvg-convert -w "$size" -h "$size" "{{icon-svg-src}}" -o "{{iconset}}/icon_${size}x${size}.png"
        double=$((size * 2))
        rsvg-convert -w "$double" -h "$double" "{{icon-svg-src}}" -o "{{iconset}}/icon_${size}x${size}@2x.png"
    done
    iconutil -c icns "{{iconset}}" -o "{{icns}}"
    rm -rf "{{iconset}}"

# Build macOS .app bundle (with embedded PWA)
[macos]
bundle: pwa-build (build-release '--features' 'embed-pwa' '-p' 'read-flow') icon
    #!/usr/bin/env bash
    set -euo pipefail
    app="{{app-bundle}}"
    rm -rf "$app"
    mkdir -p "$app/Contents/MacOS"
    mkdir -p "$app/Contents/Resources"
    cp "{{bin-src}}" "$app/Contents/MacOS/"
    cp "{{icns}}" "$app/Contents/Resources/"
    cat > "$app/Contents/Info.plist" << 'PLIST'
    <?xml version="1.0" encoding="UTF-8"?>
    <!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
    <plist version="1.0">
    <dict>
        <key>CFBundleName</key>
        <string>Read Flow</string>
        <key>CFBundleDisplayName</key>
        <string>Read Flow</string>
        <key>CFBundleIdentifier</key>
        <string>io.github.read-flow</string>
        <key>CFBundleVersion</key>
        <string>0.3.2</string>
        <key>CFBundleShortVersionString</key>
        <string>0.3.2</string>
        <key>CFBundleExecutable</key>
        <string>read-flow</string>
        <key>CFBundleIconFile</key>
        <string>ReadFlow</string>
        <key>CFBundlePackageType</key>
        <string>APPL</string>
        <key>NSHighResolutionCapable</key>
        <true/>
        <key>LSMinimumSystemVersion</key>
        <string>10.15</string>
    </dict>
    </plist>
    PLIST
    echo "Built: $app"

# Open the .app bundle (build first if needed)
[macos]
open-bundle: bundle
    open "{{app-bundle}}"

# ── PWA ───────────────────────────────────────────────────────────────────────

# Installs PWA dependencies
pwa-install:
    cd pwa && npm install

# Starts the PWA dev server
pwa-dev:
    cd pwa && npm run dev

# Builds the PWA for production
pwa-build:
    cd pwa && npm run build

# Previews the PWA production build
pwa-preview:
    cd pwa && npm run preview

# Type-checks the PWA
pwa-check:
    cd pwa && npm run check

# Runs PWA tests
pwa-test:
    cd pwa && npm test

# Runs PWA tests in watch mode
pwa-test-watch:
    cd pwa && npm run test:watch

# Runs PWA end-to-end BDD scenarios (Playwright, builds first)
pwa-test-e2e:
    cargo build -p read-flow-core --bin read-flow-cli
    cd pwa && npm run test:e2e
