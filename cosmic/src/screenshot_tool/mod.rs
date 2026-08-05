//! Headless marketing-screenshot generator for read-flow.github.io.
//!
//! Renders each scene in `docs/screenshots-needed.md` (the marketing site
//! repo) via `cosmic-golden`'s CPU renderer, at a fixed size and themed to
//! match the site's brand palette, with no display server and no live
//! network access.

mod app_harness;
mod scenes;

use std::path::Path;
use std::path::PathBuf;

use clap::Parser;
use read_flow_core::settings::FrostedStrength;
use read_flow_core::settings::ThemeDensity;
use read_flow_core::settings::ThemeRoundness;
use read_flow_core::settings::ThemeSettings;
use read_flow_core::settings::ThemeVariant;
use read_flow_core::settings::ThemeVariantSettings;

pub(super) const WIDTH: u32 = 1600;
pub(super) const HEIGHT: u32 = 1000;

/// The brand palette from `assets/brand/color-palette.md` in
/// read-flow.github.io (same values as the sample library's `[ui.theme]`
/// block), built through the same `app_theme` path the live app uses —
/// scenes must render the app as themed, not stock COSMIC dark.
pub(super) fn theme() -> cosmic::Theme {
    let settings = ThemeSettings {
        enabled: true,
        dark: ThemeVariantSettings {
            accent: Some("#a855f7".to_string()),
            background: Some("#080935".to_string()),
            container_background: Some("#151c5b".to_string()),
        },
        light: ThemeVariantSettings::default(),
        density: ThemeDensity::Standard,
        roundness: ThemeRoundness::Round,
        frosted: true,
        frosted_strength: FrostedStrength::Medium,
        interface_font: None,
        monospace_font: None,
    };
    crate::app_theme::effective_theme(&settings, ThemeVariant::Dark)
}

#[derive(Debug, clap::Parser)]
struct Args {
    /// Path to the read-flow.github.io sample library (assets/sample-library)
    #[clap(long)]
    sample_library: PathBuf,
    /// Directory to write the PNGs into (e.g. read-flow.github.io/src/assets/screenshots)
    #[clap(long)]
    out: PathBuf,
}

pub(crate) fn run() -> anyhow::Result<()> {
    let args = Args::parse();
    cosmic_golden::init();
    std::fs::create_dir_all(&args.out)?;

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    let lib: &Path = args.sample_library.as_path();
    let results: Vec<(&str, anyhow::Result<Vec<u8>>)> = rt.block_on(async {
        vec![
            ("cosmic-opds.png", scenes::opds::render(lib).await),
            (
                "cosmic-multi-instance.png",
                scenes::sources::render(lib).await,
            ),
            ("cosmic-progress.png", scenes::progress::render(lib).await),
            ("cosmic-tags-rule.png", scenes::tags_rule::render(lib).await),
            ("cosmic-tags.png", scenes::tags::render(lib).await),
            ("cosmic-scanning.png", scenes::scanning::render(lib).await),
            (
                "cosmic-epub-reader.png",
                scenes::epub_reader::render(lib).await,
            ),
            (
                "cosmic-pdf-reader.png",
                scenes::pdf_reader::render(lib).await,
            ),
        ]
    });

    let total = results.len();
    let mut failures = Vec::new();
    let mut succeeded = 0usize;
    for (filename, result) in results {
        match result.and_then(|rgba| save_png(&rgba, &args.out.join(filename))) {
            Ok(()) => {
                succeeded += 1;
                println!("wrote {filename}");
            }
            Err(e) => failures.push(format!("{filename}: {e}")),
        }
    }

    if failures.is_empty() {
        println!("{succeeded}/{total} scenes written");
        Ok(())
    } else {
        println!(
            "{succeeded}/{total} scenes written, {} failed: {}",
            failures.len(),
            failures.join("; ")
        );
        anyhow::bail!("{} scene(s) failed", failures.len());
    }
}

fn save_png(rgba: &[u8], path: &Path) -> anyhow::Result<()> {
    let image = image::RgbaImage::from_raw(WIDTH, HEIGHT, rgba.to_vec())
        .ok_or_else(|| anyhow::anyhow!("rgba buffer size mismatch for {WIDTH}x{HEIGHT}"))?;
    image.save(path)?;
    Ok(())
}
