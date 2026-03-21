// SPDX-License-Identifier: MIT

//! Golden image (snapshot) testing for libcosmic widgets.
//!
//! Each test renders a `cosmic::Element` to a PNG using the tiny-skia software
//! renderer (CPU-only, no display server required) and compares the result
//! pixel-by-pixel against a committed baseline. Any difference fails the test.
//!
//! Baselines are stored under `snapshots/<module>/<name>.png` inside the crate
//! that contains the test. The module path is derived automatically from
//! [`module_path!()`] so tests in different modules never collide even when
//! they share a function name.
//!
//! # Macros
//!
//! Three macros are provided at different levels of abstraction:
//!
//! ## `#[golden_test(width, height)]`
//!
//! The highest-level interface. Annotate a zero-argument function that returns
//! a `cosmic::Element` and it is converted into a `#[test]` automatically.
//! The snapshot name is derived from the function name. An optional third
//! argument selects the theme: `light` (default) or `dark`.
//!
//! ```rust,ignore
//! use golden::golden_test;
//!
//! #[golden_test(400, 200)]
//! fn my_widget() -> cosmic::Element<'static, ()> {
//!     cosmic::widget::text("Hello").into()
//! }
//!
//! #[golden_test(400, 200, dark)]
//! fn my_widget_dark() -> cosmic::Element<'static, ()> {
//!     cosmic::widget::text("Hello").into()
//! }
//! ```
//!
//! ## `assert_snapshot!(name, element, width, height)`
//!
//! Mid-level macro for use inside an existing `#[test]` function. Renders
//! `element` with the light theme and compares against the baseline. Useful
//! when a single test needs to produce multiple snapshots, for example with
//! `rstest` for parameterised cases.
//!
//! ```rust,no_run
//! let element: cosmic::Element<'_, ()> = cosmic::widget::text("Hello").into();
//! golden::assert_snapshot!("my_widget", element, 320, 60);
//! ```
//!
//! ## `assert_snapshot_rgba!(name, rgba, width, height)`
//!
//! Low-level primitive that operates on pre-rendered RGBA bytes. Use this when
//! you need a custom theme or want to render the element yourself via
//! [`HeadlessRenderer`].
//!
//! ```rust,no_run
//! use golden::{HeadlessRenderer, assert_snapshot_rgba};
//!
//! let mut r = HeadlessRenderer::with_theme(cosmic::Theme::dark());
//! let element: cosmic::Element<'_, ()> = cosmic::widget::text("Hello").into();
//! let rgba = r.render(element, 320, 60);
//! golden::assert_snapshot_rgba!("my_widget_dark", rgba, 320, 60);
//! ```
//!
//! # Updating baselines
//!
//! Set the `UPDATE_SNAPSHOTS` environment variable to regenerate baselines
//! instead of comparing:
//!
//! ```bash
//! UPDATE_SNAPSHOTS=1 cargo nextest run -p my-crate
//! ```

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
