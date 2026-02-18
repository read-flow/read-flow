use std::io;

#[derive(Debug, thiserror::Error)]
pub enum EpubError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("ZIP error: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("XML error: {0}")]
    Xml(#[from] quick_xml::Error),

    #[error("missing file in archive: {0}")]
    MissingFile(String),

    #[error("invalid container.xml: {0}")]
    InvalidContainer(String),

    #[error("invalid OPF package: {0}")]
    InvalidPackage(String),

    #[error("resource not found: {0}")]
    ResourceNotFound(String),
}

pub type Result<T> = std::result::Result<T, EpubError>;
