use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpineItem {
    pub index: usize,
    pub id: String,
    pub href: String,
    pub linear: bool,
    /// Human-readable label from the Navigation Document or NCX, if available.
    #[serde(default)]
    pub label: Option<String>,
}
