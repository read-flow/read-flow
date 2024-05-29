use std::path::PathBuf;

use anyhow::Result;
use thiserror::Error;

#[derive(Debug, Error)]
enum Error {
    #[error("Supplied path `{0}` is not a directory")]
    NotADirectory(PathBuf)
}

pub fn scan(directory: PathBuf) -> Result<()> {
    if !directory.is_dir() {
	return Err(Error::NotADirectory(directory))?;
    }

    Ok(())
}
