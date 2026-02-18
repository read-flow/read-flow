use crate::domain::metadata::DocumentMetadata;
use crate::domain::spine::SpineItem;
use crate::error::Result;

pub trait Document {
    fn id(&self) -> &str;
    fn metadata(&self) -> &DocumentMetadata;
    fn spine(&self) -> &[SpineItem];
    fn resolve_resource(&self, href: &str) -> Result<Vec<u8>>;
}
