use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpineItem {
    pub index: usize,
    pub id: String,
    pub href: String,
    pub linear: bool,
}
