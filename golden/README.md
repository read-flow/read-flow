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
fn my_widget_looks_correct() -> cosmic::Element<'static, ()> {
    cosmic::widget::text("Hello, world!").into()
}
```

The macro:
- derives the snapshot name from the **function name** (`my_widget_looks_correct`)
- wraps the function body in a `#[test]`
- calls `assert_snapshot!` with the given pixel dimensions

### Using `assert_snapshot!` directly

For cases where you need more control (e.g. testing several variants in one test function):

```rust
use golden::assert_snapshot;

#[test]
fn widget_variants() {
    let light: cosmic::Element<'_, ()> = my_widget(Theme::light()).into();
    assert_snapshot!("my_widget_light", light, 400, 200);

    let dark: cosmic::Element<'_, ()> = my_widget(Theme::dark()).into();
    assert_snapshot!("my_widget_dark", dark, 400, 200);
}
```

`assert_snapshot!(name, element, width, height)` where:

- **`name`** — logical snapshot name; the stored file will be `snapshots/<name>.png`.
- **`element`** — any `cosmic::Element<'_, Message>`.
- **`width` / `height`** — pixel dimensions of the rendered viewport.

## Generated files

| File                          | When created                       | Purpose                                           |
|-------------------------------|------------------------------------|---------------------------------------------------|
| `snapshots/<name>.png`        | First run, or `UPDATE_SNAPSHOTS=1` | Committed baseline                                |
| `snapshots/<name>.actual.png` | On mismatch                        | Rendered output for inspection; **not** committed |

On the **first run** (no baseline exists yet) the test passes and writes the baseline
automatically. Commit the new PNG to make it part of the test suite.

## When a test fails

If a test fails you will see:

```
golden: snapshot 'my_widget_looks_correct' differs by 312 pixels.
Actual saved to "golden/snapshots/my_widget_looks_correct.actual.png".
Run with UPDATE_SNAPSHOTS=1 to regenerate.
```

Open both files side-by-side to inspect the difference:

- `snapshots/my_widget_looks_correct.png` — the committed baseline
- `snapshots/my_widget_looks_correct.actual.png` — what the renderer produced this run

The `.actual.png` file is not tracked by git.

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
UPDATE_SNAPSHOTS=1 cargo nextest run -p golden -- my_widget_looks_correct
```
