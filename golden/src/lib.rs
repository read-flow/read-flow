// SPDX-License-Identifier: MIT

pub mod renderer;
pub mod snapshot;

pub use golden_macros::golden_test;
pub use renderer::HeadlessRenderer;

/// Compare pre-rendered RGBA bytes against the stored PNG baseline.
///
/// This is the low-level primitive used by [`assert_snapshot!`] and by the
/// `#[golden_test]` expansion. Prefer those over calling this directly.
///
/// The snapshot is stored under `<crate-root>/snapshots/<module>/<name>.png`,
/// where `<crate-root>` is the root of the crate that invokes the macro and
/// `<module>` mirrors the caller's Rust module path. This keeps baselines
/// co-located with the crate under test and prevents name collisions between
/// tests in different modules.
#[macro_export]
macro_rules! assert_snapshot_rgba {
    ($name:expr, $rgba:expr, $width:expr, $height:expr $(,)?) => {{
        let module_subdir = module_path!().replace("::", "/");
        let base = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("snapshots")
            .join(&module_subdir)
            .join($name);
        let png_path = base.with_extension("png");

        if std::env::var("UPDATE_SNAPSHOTS").is_ok() {
            $crate::snapshot::save_png(&png_path, &$rgba, $width, $height);
            eprintln!("golden: updated snapshot {:?}", png_path);
        } else if png_path.exists() {
            let (expected, w, h) = $crate::snapshot::load_png(&png_path);
            assert_eq!(
                ($width, $height),
                (w, h),
                "golden: size mismatch for '{}': stored {}×{}, rendered {}×{}",
                $name,
                w,
                h,
                $width,
                $height,
            );
            let diff = $crate::snapshot::count_differing_pixels(&$rgba, &expected);
            if diff > 0 {
                let actual_path = base.with_extension("actual.png");
                let diff_path = base.with_extension("diff.png");
                $crate::snapshot::save_png(&actual_path, &$rgba, $width, $height);
                let diff_image = $crate::snapshot::diff_image(&$rgba, &expected);
                $crate::snapshot::save_png(&diff_path, &diff_image, $width, $height);
                panic!(
                    "golden: snapshot '{}' differs by {} pixels.\n\
                     Actual: {:?}\n\
                     Diff:   {:?}\n\
                     Run with UPDATE_SNAPSHOTS=1 to regenerate.",
                    $name, diff, actual_path, diff_path,
                );
            }
        } else {
            // No baseline yet — save it and pass.
            $crate::snapshot::save_png(&png_path, &$rgba, $width, $height);
            eprintln!("golden: created initial snapshot {:?}", png_path);
        }
    }};
}

/// Assert that rendering `element` with the light theme matches the stored PNG baseline.
///
/// The baseline is stored at `snapshots/<module>/<name>.png` relative to
/// the crate that contains the golden tests.
///
/// Set `UPDATE_SNAPSHOTS=1` to regenerate baselines instead of comparing.
///
/// For dark-theme tests or other custom themes, use `#[golden_test(w, h, dark)]`
/// or construct a [`HeadlessRenderer`] with [`HeadlessRenderer::with_theme`] directly.
///
/// # Usage
///
/// ```rust,no_run
/// let element: cosmic::Element<'_, ()> = cosmic::widget::text("Hello").into();
/// golden::assert_snapshot!("my_widget", element, 320, 60);
/// ```
#[macro_export]
macro_rules! assert_snapshot {
    ($name:expr, $element:expr, $width:expr, $height:expr $(,)?) => {{
        let mut r = $crate::HeadlessRenderer::new();
        let rgba = r.render($element, $width, $height);
        $crate::assert_snapshot_rgba!($name, rgba, $width, $height);
    }};
}
