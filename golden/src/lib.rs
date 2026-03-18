// SPDX-License-Identifier: GPL-3.0-or-later

pub mod renderer;
pub mod snapshot;

pub use golden_macros::golden_test;
pub use renderer::HeadlessRenderer;

/// Assert that rendering `element` at the given pixel size matches the stored PNG baseline.
///
/// The baseline is stored at `golden/snapshots/<name>.png`.
///
/// Set `UPDATE_SNAPSHOTS=1` to regenerate baselines instead of comparing.
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

        let png_path = $crate::snapshot::snapshots_dir()
            .join($name)
            .with_extension("png");

        if std::env::var("UPDATE_SNAPSHOTS").is_ok() {
            $crate::snapshot::save_png(&png_path, &rgba, $width, $height);
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
            let diff = $crate::snapshot::count_differing_pixels(&rgba, &expected);
            if diff > 0 {
                let actual_path = $crate::snapshot::snapshots_dir()
                    .join($name)
                    .with_extension("actual.png");
                $crate::snapshot::save_png(&actual_path, &rgba, $width, $height);
                panic!(
                    "golden: snapshot '{}' differs by {} pixels.\n\
                     Actual saved to {:?}.\n\
                     Run with UPDATE_SNAPSHOTS=1 to regenerate.",
                    $name, diff, actual_path,
                );
            }
        } else {
            // No baseline yet — save it and pass.
            $crate::snapshot::save_png(&png_path, &rgba, $width, $height);
            eprintln!("golden: created initial snapshot {:?}", png_path);
        }
    }};
}
