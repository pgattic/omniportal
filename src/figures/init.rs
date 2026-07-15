pub const DEFAULT_ENTITY_NAME: &str = "Fresh Entity";

use crate::figures::formats::SKYLANDERS_IMAGE_BYTES;

pub fn initialize_skylanders_placeholder(
    character_id: u32,
    variant_id: Option<u32>,
) -> [u8; SKYLANDERS_IMAGE_BYTES] {
    let mut image = [0xff; SKYLANDERS_IMAGE_BYTES];
    image[0..4].copy_from_slice(b"OMNI");
    image[4..8].copy_from_slice(&character_id.to_le_bytes());
    image[8..12].copy_from_slice(&variant_id.unwrap_or(0).to_le_bytes());
    image[12] = u8::from(variant_id.is_some());
    image
}
