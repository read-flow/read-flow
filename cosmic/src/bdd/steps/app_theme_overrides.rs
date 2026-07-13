//! Steps for `features/app_theme_overrides.feature`.
use cucumber::then;
use cucumber::when;

use crate::bdd::world::BddWorld;

#[when("I enable the custom app theme")]
async fn enable_custom_theme(world: &mut BddWorld) {
    world.driver.set_custom_theme_enabled(true).await;
}

#[when("I disable the custom app theme")]
async fn disable_custom_theme(world: &mut BddWorld) {
    world.driver.set_custom_theme_enabled(false).await;
}

#[when(regex = r#"^I set the app theme variant to "([^"]+)"$"#)]
async fn set_theme_variant(world: &mut BddWorld, variant: String) {
    world.driver.set_theme_variant(&variant).await;
}

#[when(regex = r#"^I set the app accent color to "([^"]+)"$"#)]
async fn set_accent_color(world: &mut BddWorld, hex: String) {
    world.driver.set_theme_accent(&hex).await;
}

#[then("the effective app theme is dark")]
async fn effective_theme_is_dark(world: &mut BddWorld) {
    assert!(
        world.driver.effective_theme_is_dark(),
        "expected the effective app theme to be dark"
    );
}

#[then(regex = r#"^the effective accent color is "([^"]+)"$"#)]
async fn effective_accent_color(world: &mut BddWorld, hex: String) {
    let actual = world.driver.effective_accent_hex();
    assert_eq!(actual, hex, "expected accent {hex:?}, got {actual:?}");
}

#[then("the app follows the system theme")]
async fn follows_system_theme(world: &mut BddWorld) {
    assert!(
        world.driver.follows_system_theme(),
        "expected the app to follow the system theme"
    );
}
