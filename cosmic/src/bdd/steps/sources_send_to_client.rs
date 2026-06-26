//! Steps for `features/sources_send_to_client.feature`.
use cucumber::then;
use cucumber::when;

use crate::bdd::world::BddWorld;

// `Given a read-flow server is running…` is in common.rs.

#[when("I send a document to the server")]
async fn send_document_to_server(world: &mut BddWorld) {
    let ok = world.driver.send_document_to_server().await;
    world.last_check = Some(ok);
}

#[then("the document was accepted by the server")]
async fn document_accepted(world: &mut BddWorld) {
    let accepted = world
        .last_check
        .expect("send_document_to_server must run first");
    assert!(accepted, "server rejected the document upload");
}
