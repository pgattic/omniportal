pub mod formats;
pub mod infinity;
pub mod skylanders;

pub fn initialize() {
    let _ = formats::MAX_FIGURE_IMAGE_BYTES;
    let _ = skylanders::SKYLANDERS_CATALOG.len();
    let _ = infinity::INFINITY_CATALOG.len();
    let _ = skylanders::image::DEFAULT_ENTITY_NAME;
    let _ = skylanders::crypto::FIGURE_SIZE;
}
