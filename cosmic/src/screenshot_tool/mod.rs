//! Headless marketing-screenshot generator for read-flow.github.io.
//!
//! Renders each scene in `docs/screenshots-needed.md` (the marketing site
//! repo) via `cosmic-golden`'s CPU renderer, at a fixed size and dark theme,
//! with no display server and no live network access.

mod scenes;

use std::path::Path;
use std::path::PathBuf;

use clap::Parser;

pub(super) const WIDTH: u32 = 1600;
pub(super) const HEIGHT: u32 = 1000;

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
