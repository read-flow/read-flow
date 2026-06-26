//! Steps for `features/reading_image_viewer.feature`.
use cucumber::then;
use cucumber::when;

use crate::bdd::world::BddWorld;

// `Given a read-flow server is running…` is in common.rs.

#[when("I open an image in the viewer")]
async fn open_image_in_viewer(world: &mut BddWorld) {
    world.reading_open = true;
}

#[then("the image is displayed")]
async fn image_is_displayed(world: &mut BddWorld) {
    let opened = world.driver.image_viewer_opens_successfully();
    assert!(
        opened,
        "ImageViewer could not be instantiated with test image data"
    );
}
