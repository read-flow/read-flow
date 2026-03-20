# golden

Snapshot (golden image) testing for libcosmic widgets.

Each test renders a cosmic widget tree to a PNG using the tiny-skia software renderer
(CPU-only, no display server required) and compares the result against a committed baseline.
Tests fail if any pixel differs.

## Writing a test

The preferred way is the `#[golden_test(width, height)]` attribute macro. Annotate any
zero-argument function that returns a `cosmic::Element`:

```rust
use golden::golden_test;

#[golden_test(400, 200)]
fn my_widget_light() -> cosmic::Element<'static, ()> {
    my_widget().into()
}
```

An optional third argument selects the theme — `light` (default) or `dark`:

```rust
#[golden_test(400, 200, dark)]
fn my_widget_dark() -> cosmic::Element<'static, ()> {
    my_widget().into()
}
```

The macro:
- derives the snapshot name from the **function name**
- wraps the function body in a `#[test]`
- renders with the chosen theme and compares against the stored baseline

Name the function to reflect the theme variant when testing both, so each gets
its own snapshot file (`my_widget_light.png` / `my_widget_dark.png`).

### Snapshot paths and module namespacing

Snapshots are stored inside the **snapshots directory in the crate**. The
directory structure mirrors the Rust module path of the test, so tests in
different modules never collide even when they share a function name:

```
<your-crate>/snapshots/<module>/<name>.png
```

For example, a test `view_pagination` in `read_flow::component::pagination::tests` produces:

```
read-flow/snapshots/read_flow/component/pagination/tests/view_pagination.png
```

The path is derived automatically from `module_path!()` and `env!("CARGO_MANIFEST_DIR")`
at the call site — no manual namespacing is needed.

### Using `assert_snapshot!` directly

For cases where you need more control within a single test function, construct a
[`HeadlessRenderer`] with the desired theme and use `assert_snapshot_rgba!`:

```rust
use golden::{HeadlessRenderer, assert_snapshot_rgba};

#[test]
fn both_themes() {
    for (name, theme) in [
        ("my_widget_dark",  cosmic::Theme::dark()),
        ("my_widget_light", cosmic::Theme::light()),
    ] {
        let element: cosmic::Element<'_, ()> = my_widget().into();
        let mut r = HeadlessRenderer::with_theme(theme);
        let rgba = r.render(element, 400, 200);
        assert_snapshot_rgba!(name, rgba, 400, 200);
    }
}
```

`assert_snapshot!(name, element, width, height)` is a shorthand that always uses the light
theme. It is equivalent to the above with `HeadlessRenderer::new()`.

## Generated files

`<crate-root>` is the root of the crate containing the test.
`<module>` is the caller's Rust module path with `::` replaced by `/`.

| File                                    | When created                       | Purpose                                           |
|-----------------------------------------|------------------------------------|---------------------------------------------------|
| `snapshots/<module>/<name>.png`         | First run, or `UPDATE_SNAPSHOTS=1` | Committed baseline                                |
| `snapshots/<module>/<name>.actual.png`  | On mismatch                        | Rendered output for inspection; **not** committed |
| `snapshots/<module>/<name>.diff.png`    | On mismatch                        | Amplified per-channel delta; **not** committed    |

On the **first run** (no baseline exists yet) the test passes and writes the baseline
automatically. Commit the new PNG to make it part of the test suite.

## When a test fails

If a test fails you will see:

```
golden: snapshot 'my_widget_dark' differs by 312 pixels.
Actual: "my-crate/snapshots/my_crate/tests/smoke_test/my_widget_dark.actual.png"
Diff:   "my-crate/snapshots/my_crate/tests/smoke_test/my_widget_dark.diff.png"
Run with UPDATE_SNAPSHOTS=1 to regenerate.
```

Three files are available for inspection:

- `snapshots/my_crate/tests/smoke_test/my_widget_dark.png` — the committed baseline
- `snapshots/my_crate/tests/smoke_test/my_widget_dark.actual.png` — what the renderer produced this run
- `snapshots/my_crate/tests/smoke_test/my_widget_dark.diff.png` — per-channel absolute difference amplified 10×;
  black means identical, bright colours indicate where and how much pixels differ

The `.actual.png` and `.diff.png` files should not be committed to git.

## Updating baselines

After verifying that the visual change is intentional:

```bash
UPDATE_SNAPSHOTS=1 cargo nextest run -p golden
```

This overwrites every baseline PNG with the current render output. Review the changed images,
then commit:

```bash
git add golden/snapshots/
git commit -m "chore: update golden image baselines"
```

To regenerate only one snapshot, run its test by name:

```bash
UPDATE_SNAPSHOTS=1 cargo nextest run -p golden -- my_widget_dark
```
