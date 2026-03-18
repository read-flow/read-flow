use std::io::BufReader;
use std::io::BufWriter;
use std::path::Path;
use std::path::PathBuf;

/// Returns the absolute path to the `snapshots/` directory of the `golden` crate.
pub fn snapshots_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("snapshots")
}

/// Encodes RGBA bytes as a PNG file at `path`.
pub fn save_png(path: &Path, rgba: &[u8], width: u32, height: u32) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create snapshot directory");
    }
    let file =
        std::fs::File::create(path).unwrap_or_else(|e| panic!("create snapshot PNG {path:?}: {e}"));
    let mut enc = png::Encoder::new(BufWriter::new(file), width, height);
    enc.set_compression(png::Compression::Balanced);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .expect("write PNG header")
        .write_image_data(rgba)
        .expect("write PNG data");
}

/// Decodes a PNG file into `(rgba_bytes, width, height)`.
pub fn load_png(path: &Path) -> (Vec<u8>, u32, u32) {
    let file =
        std::fs::File::open(path).unwrap_or_else(|e| panic!("open snapshot PNG {path:?}: {e}"));
    let dec = png::Decoder::new(BufReader::new(file));
    let mut reader = dec
        .read_info()
        .unwrap_or_else(|e| panic!("read PNG info {path:?}: {e}"));
    let n = reader.output_buffer_size().expect("PNG fits in memory");
    let mut buf = vec![0u8; n];
    let info = reader
        .next_frame(&mut buf)
        .unwrap_or_else(|e| panic!("decode PNG {path:?}: {e}"));
    buf.truncate(info.buffer_size());
    (buf, info.width, info.height)
}

/// Returns the number of pixels that differ between two RGBA byte slices.
pub fn count_differing_pixels(a: &[u8], b: &[u8]) -> usize {
    a.chunks(4)
        .zip(b.chunks(4))
        .filter(|(pa, pb)| pa != pb)
        .count()
}
