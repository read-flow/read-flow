//! Steps for `features/app_epub_viewer_choice.feature`.
use cucumber::then;
use cucumber::when;

use crate::bdd::world::BddWorld;

#[when(regex = r#"^I select "([^"]+)" as the EPUB viewer$"#)]
async fn select_epub_viewer(world: &mut BddWorld, choice: String) {
    world.driver.set_epub_viewer_choice(&choice).await;
}

#[then(regex = r#"^"([^"]+)" is the active EPUB viewer$"#)]
async fn epub_viewer_is_active(world: &mut BddWorld, choice: String) {
    let actual = world.driver.epub_viewer_choice().to_string();
    assert_eq!(
        actual, choice,
        "expected EPUB viewer {choice:?}, got {actual:?}"
    );
}
