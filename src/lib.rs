pub mod client;
pub mod db;
pub mod gui;
pub mod scan;
pub mod serve;

use std::path::PathBuf;

fn to_unique_file(file_path: &mut PathBuf, extension: &str) {
    let mut index: usize = 1;

    while file_path.exists() {
        if index > 1 {
            file_path.set_extension("");
        }
        file_path.set_extension(format!("{index}.{extension}"));
        index += 1;
    }
}

fn extension_of(filename: &str) -> Option<&str> {
    filename.split(".").last()
}
