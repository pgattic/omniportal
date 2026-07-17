pub const DEFAULT_ENTITY_NAME: &str = "Fresh Entity";

use crate::figures::formats::SKYLANDERS_IMAGE_BYTES;

pub fn initialize_skylanders_placeholder(
    character_id: u32,
    variant_id: Option<u32>,
) -> [u8; SKYLANDERS_IMAGE_BYTES] {
    initialize_skylanders_placeholder_with_uid(character_id, variant_id, *b"OMNI")
}

pub fn initialize_skylanders_entity_image(
    character_id: u32,
    variant_id: Option<u32>,
    entity_id: u32,
) -> [u8; SKYLANDERS_IMAGE_BYTES] {
    initialize_skylanders_placeholder_with_uid(
        character_id,
        variant_id,
        generated_uid(character_id, variant_id, entity_id),
    )
}

pub fn rekey_skylanders_entity_image(
    image: &mut [u8],
    character_id: u32,
    variant_id: Option<u32>,
    entity_id: u32,
) -> bool {
    if image.len() != SKYLANDERS_IMAGE_BYTES {
        return false;
    }

    let uid = generated_uid(character_id, variant_id, entity_id);
    image[0..4].copy_from_slice(&uid);
    image[4] = image[0] ^ image[1] ^ image[2] ^ image[3];
    populate_header_fields(image, character_id, variant_id);

    let crc = crc16(&image[..0x1e]);
    image[0x1e..0x20].copy_from_slice(&crc.to_le_bytes());

    let nuid = [image[0], image[1], image[2], image[3]];
    for sector in 0..0x10 {
        let key = calculate_key_a(sector as u8, nuid);
        let offset = sector * 0x40 + 0x30;
        for index in 0..6 {
            image[offset + index] = (key >> ((5 - index) * 8)) as u8;
        }
    }

    true
}

pub fn initialize_skylanders_placeholder_with_uid(
    character_id: u32,
    variant_id: Option<u32>,
    uid: [u8; 4],
) -> [u8; SKYLANDERS_IMAGE_BYTES] {
    let mut image = [0; SKYLANDERS_IMAGE_BYTES];
    image[0..4].copy_from_slice(&uid);
    image[4] = image[0] ^ image[1] ^ image[2] ^ image[3];
    image[5] = 0x81;
    image[6] = 0x01;
    image[7] = 0x0f;

    populate_sector_trailers(&mut image);
    populate_header_fields(&mut image, character_id, variant_id);
    compute_checksum_type0(&mut image);
    populate_keys(&mut image);
    image
}

fn generated_uid(character_id: u32, variant_id: Option<u32>, entity_id: u32) -> [u8; 4] {
    let variant = variant_id.unwrap_or(0);
    let mut seed = 0x4f4d_4e49u32;
    seed ^= entity_id.wrapping_mul(0x9e37_79b1);
    seed ^= character_id.rotate_left(7);
    seed ^= variant.rotate_left(19);
    let mut uid = seed.to_le_bytes();
    uid[0] = (uid[0] & 0xfe) | 0x04;
    uid
}

fn populate_header_fields(image: &mut [u8], character_id: u32, variant_id: Option<u32>) {
    image[0x10..0x12].copy_from_slice(&(character_id as u16).to_le_bytes());
    image[0x12..0x1c].fill(0);
    image[0x1c..0x1e].copy_from_slice(&(variant_id.unwrap_or(0) as u16).to_le_bytes());
}

fn populate_sector_trailers(image: &mut [u8; SKYLANDERS_IMAGE_BYTES]) {
    image[0x36..0x3a].copy_from_slice(&0x690f_0f0fu32.to_le_bytes());
    for sector in 1..0x10 {
        let offset = sector * 0x40 + 0x36;
        image[offset..offset + 4].copy_from_slice(&0x6908_0f7fu32.to_le_bytes());
    }
}

fn populate_keys(image: &mut [u8; SKYLANDERS_IMAGE_BYTES]) {
    let nuid = [image[0], image[1], image[2], image[3]];
    for sector in 0..0x10 {
        let key = calculate_key_a(sector as u8, nuid);
        let offset = sector * 0x40 + 0x30;
        for index in 0..6 {
            image[offset + index] = (key >> ((5 - index) * 8)) as u8;
        }
    }
}

fn calculate_key_a(sector: u8, nuid: [u8; 4]) -> u64 {
    if sector == 0 {
        return 73 * 2017 * 560_381_651;
    }

    let crc = crc48(&[nuid[0], nuid[1], nuid[2], nuid[3], sector]);
    crc.swap_bytes() >> 16
}

fn compute_checksum_type0(image: &mut [u8; SKYLANDERS_IMAGE_BYTES]) {
    let crc = crc16(&image[..0x1e]);
    image[0x1e..0x20].copy_from_slice(&crc.to_le_bytes());
}

fn crc16(data: &[u8]) -> u16 {
    let mut crc = 0xffffu16;
    for byte in data {
        crc ^= (*byte as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

fn crc48(data: &[u8]) -> u64 {
    const POLYNOMIAL: u64 = 0x42f0_e1eb_a9ea_3693;
    let mut crc = 2 * 2 * 3 * 1103 * 12_868_356_821u64;
    for byte in data {
        crc ^= (*byte as u64) << 40;
        for _ in 0..8 {
            crc = if crc & (1 << 47) != 0 {
                (crc << 1) ^ POLYNOMIAL
            } else {
                crc << 1
            };
        }
    }
    crc & 0x0000_ffff_ffff_ffff
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_skylanders_image_matches_dolphin_create_layout() {
        let image = initialize_skylanders_placeholder(19, None);

        assert_eq!(&image[0..4], b"OMNI");
        assert_eq!(image[4], b'O' ^ b'M' ^ b'N' ^ b'I');
        assert_eq!(&image[5..8], &[0x81, 0x01, 0x0f]);
        assert_eq!(&image[0x10..0x12], &19u16.to_le_bytes());
        assert_eq!(image[0x12], 0);
        assert_eq!(image[0x13], 0);
        assert_eq!(&image[0x14..0x1c], &[0; 8]);
        assert_eq!(&image[0x1c..0x1e], &0u16.to_le_bytes());
        assert_eq!(&image[0x1e..0x20], &crc16(&image[..0x1e]).to_le_bytes());
        assert_eq!(&image[0x36..0x3a], &0x690f_0f0fu32.to_le_bytes());
        assert_eq!(&image[0x76..0x7a], &0x6908_0f7fu32.to_le_bytes());
    }

    #[test]
    fn generated_entity_images_have_stable_distinct_uids() {
        let first = initialize_skylanders_entity_image(21, None, 1);
        let second = initialize_skylanders_entity_image(21, None, 2);
        let first_again = initialize_skylanders_entity_image(21, None, 1);

        assert_eq!(&first[0..4], &first_again[0..4]);
        assert_ne!(&first[0..4], &second[0..4]);
        assert_eq!(first[4], first[0] ^ first[1] ^ first[2] ^ first[3]);
        assert_eq!(&first[0x1e..0x20], &crc16(&first[..0x1e]).to_le_bytes());
        assert_ne!(&first[0x70..0x76], &second[0x70..0x76]);
    }

    #[test]
    fn rekey_updates_uid_checksum_and_sector_keys() {
        let mut image = initialize_skylanders_entity_image(21, None, 1);
        let old_uid = image[0..4].to_vec();
        let old_key = image[0x70..0x76].to_vec();
        image[0x14..0x1c].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);

        assert!(rekey_skylanders_entity_image(&mut image, 21, None, 2));

        assert_ne!(&image[0..4], old_uid.as_slice());
        assert_ne!(&image[0x70..0x76], old_key.as_slice());
        assert_eq!(image[4], image[0] ^ image[1] ^ image[2] ^ image[3]);
        assert_eq!(&image[0x14..0x1c], &[0; 8]);
        assert_eq!(&image[0x1e..0x20], &crc16(&image[..0x1e]).to_le_bytes());
    }

    #[test]
    fn generated_header_matches_dolphin_create_id_layout() {
        let image = initialize_skylanders_placeholder(0x12_3456, Some(0x0789));

        assert_eq!(&image[0x10..0x12], &0x3456u16.to_le_bytes());
        assert_eq!(image[0x12], 0);
        assert_eq!(image[0x13], 0);
        assert_eq!(&image[0x14..0x1c], &[0; 8]);
        assert_eq!(&image[0x1c..0x1e], &0x0789u16.to_le_bytes());
        assert_eq!(&image[0x1e..0x20], &crc16(&image[..0x1e]).to_le_bytes());
    }
}
