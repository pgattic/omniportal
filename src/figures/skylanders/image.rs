pub const DEFAULT_ENTITY_NAME: &str = "Fresh Entity";

use crate::domain::FigureKind;
use crate::figures::formats::SKYLANDERS_IMAGE_BYTES;
use crate::figures::skylanders::crypto::{
    calculate_key_a, checksum_type0, checksum_type1, checksum_type2, checksum_type3,
    checksum_type6, decrypt_figure, encrypt_figure, FIRST_SECTOR_TRAILER_ACL,
    OTHER_SECTOR_TRAILER_ACL,
};

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

pub fn initialize_mutable_skylanders_entity_image(
    character_id: u32,
    variant_id: Option<u32>,
    entity_id: u32,
    kind: FigureKind,
) -> [u8; SKYLANDERS_IMAGE_BYTES] {
    let mut image = initialize_skylanders_entity_image(character_id, variant_id, entity_id);
    initialize_default_save_data(&mut image, kind);
    image
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

    let should_encrypt = has_nonzero_encrypted_data_blocks(image);
    let mut working = [0; SKYLANDERS_IMAGE_BYTES];
    working.copy_from_slice(image);
    working = decrypt_figure(&working);

    let uid = generated_uid(character_id, variant_id, entity_id);
    working[0..4].copy_from_slice(&uid);
    working[4] = working[0] ^ working[1] ^ working[2] ^ working[3];
    populate_header_fields(&mut working, character_id, variant_id);

    let crc = checksum_type0(&working).expect("skylanders image header is present");
    working[0x1e..0x20].copy_from_slice(&crc.to_le_bytes());

    let nuid = [working[0], working[1], working[2], working[3]];
    for sector in 0..0x10 {
        let key = calculate_key_a(sector as u8, nuid);
        let offset = sector * 0x40 + 0x30;
        for index in 0..6 {
            working[offset + index] = (key >> ((5 - index) * 8)) as u8;
        }
    }

    if should_encrypt {
        image.copy_from_slice(&encrypt_figure(&working));
    } else {
        image.copy_from_slice(&working);
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
    image[0x36..0x3a].copy_from_slice(&FIRST_SECTOR_TRAILER_ACL.to_le_bytes());
    for sector in 1..0x10 {
        let offset = sector * 0x40 + 0x36;
        image[offset..offset + 4].copy_from_slice(&OTHER_SECTOR_TRAILER_ACL.to_le_bytes());
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

fn compute_checksum_type0(image: &mut [u8; SKYLANDERS_IMAGE_BYTES]) {
    let crc = checksum_type0(image).expect("skylanders image header is present");
    image[0x1e..0x20].copy_from_slice(&crc.to_le_bytes());
}

fn initialize_default_save_data(image: &mut [u8; SKYLANDERS_IMAGE_BYTES], kind: FigureKind) {
    match kind {
        FigureKind::Character
        | FigureKind::Trap
        | FigureKind::CreationCrystal
        | FigureKind::Vehicle => initialize_character_save_data(image),
        FigureKind::Trophy => initialize_trophy_save_data(image),
        FigureKind::Item | FigureKind::LevelPiece | FigureKind::Unknown => {}
    }
}

fn initialize_character_save_data(image: &mut [u8; SKYLANDERS_IMAGE_BYTES]) {
    let mut plaintext = decrypt_figure(image);
    let area_offset = primary_area_to_write(&plaintext);
    let other_area_offset = paired_primary_area(area_offset);

    write_u16(&mut plaintext, area_offset + 0x5a, 1);
    write_primary_area_checksums(&mut plaintext, area_offset, other_area_offset);
    write_secondary_area_checksums(&mut plaintext);

    *image = encrypt_figure(&plaintext);
}

fn initialize_trophy_save_data(image: &mut [u8; SKYLANDERS_IMAGE_BYTES]) {
    let mut plaintext = decrypt_figure(image);
    let area_offset = primary_area_to_write(&plaintext);
    let other_area_offset = paired_primary_area(area_offset);

    plaintext[area_offset + 0x14] = 0;
    write_primary_area_checksums(&mut plaintext, area_offset, other_area_offset);
    write_secondary_area_checksums(&mut plaintext);

    *image = encrypt_figure(&plaintext);
}

fn primary_area_to_write(image: &[u8; SKYLANDERS_IMAGE_BYTES]) -> usize {
    if image[0x89] != image[0x249].wrapping_add(1) {
        0x80
    } else {
        0x240
    }
}

fn paired_primary_area(area_offset: usize) -> usize {
    if area_offset == 0x80 {
        0x240
    } else {
        0x80
    }
}

fn secondary_area_to_write(image: &[u8; SKYLANDERS_IMAGE_BYTES]) -> usize {
    if image[0x112] != image[0x2d2].wrapping_add(1) {
        0x110
    } else {
        0x2d0
    }
}

fn paired_secondary_area(area_offset: usize) -> usize {
    if area_offset == 0x110 {
        0x2d0
    } else {
        0x110
    }
}

fn write_primary_area_checksums(
    image: &mut [u8; SKYLANDERS_IMAGE_BYTES],
    area_offset: usize,
    other_area_offset: usize,
) {
    let checksum3 = checksum_type3(&image[area_offset + 0x50..])
        .expect("skylanders primary save area has checksum3 input");
    image[area_offset + 0x0a..area_offset + 0x0c].copy_from_slice(&checksum3.to_le_bytes());

    let checksum2 = checksum_type2(&image[area_offset + 0x10..])
        .expect("skylanders primary save area has checksum2 input");
    image[area_offset + 0x0c..area_offset + 0x0e].copy_from_slice(&checksum2.to_le_bytes());

    image[area_offset + 0x09] = image[other_area_offset + 0x09].wrapping_add(1);

    let checksum1 = checksum_type1(&image[area_offset..])
        .expect("skylanders primary save area has checksum1 input");
    image[area_offset + 0x0e..area_offset + 0x10].copy_from_slice(&checksum1.to_le_bytes());
}

fn write_secondary_area_checksums(image: &mut [u8; SKYLANDERS_IMAGE_BYTES]) {
    let area_offset = secondary_area_to_write(image);
    let other_area_offset = paired_secondary_area(area_offset);
    image[area_offset + 0x02] = image[other_area_offset + 0x02].wrapping_add(1);

    let checksum6 = checksum_type6(&image[area_offset..])
        .expect("skylanders secondary save area has checksum6 input");
    image[area_offset..area_offset + 0x02].copy_from_slice(&checksum6.to_le_bytes());
}

fn write_u16(image: &mut [u8; SKYLANDERS_IMAGE_BYTES], offset: usize, value: u16) {
    image[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn has_nonzero_encrypted_data_blocks(image: &[u8]) -> bool {
    for block_index in 8..0x40 {
        if block_index % 4 == 3 {
            continue;
        }
        let start = block_index * 0x10;
        let Some(block) = image.get(start..start + 0x10) else {
            return false;
        };
        if block.iter().any(|byte| *byte != 0) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::figures::skylanders::crypto::{
        crc16_ccitt_false, validate_generated_image, BLOCK_COUNT, BLOCK_SIZE,
    };

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
        assert_eq!(
            &image[0x1e..0x20],
            &crc16_ccitt_false(&image[..0x1e]).to_le_bytes()
        );
        assert_eq!(&image[0x36..0x3a], &0x690f_0f0fu32.to_le_bytes());
        assert_eq!(&image[0x76..0x7a], &0x6908_0f7fu32.to_le_bytes());
        assert_eq!(validate_generated_image(&image), Ok(()));
    }

    #[test]
    fn generated_entity_images_have_stable_distinct_uids() {
        let first = initialize_skylanders_entity_image(21, None, 1);
        let second = initialize_skylanders_entity_image(21, None, 2);
        let first_again = initialize_skylanders_entity_image(21, None, 1);

        assert_eq!(&first[0..4], &first_again[0..4]);
        assert_ne!(&first[0..4], &second[0..4]);
        assert_eq!(first[4], first[0] ^ first[1] ^ first[2] ^ first[3]);
        assert_eq!(
            &first[0x1e..0x20],
            &crc16_ccitt_false(&first[..0x1e]).to_le_bytes()
        );
        assert_ne!(&first[0x70..0x76], &second[0x70..0x76]);
        assert_eq!(validate_generated_image(&first), Ok(()));
        assert_eq!(validate_generated_image(&second), Ok(()));
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
        assert_eq!(
            &image[0x1e..0x20],
            &crc16_ccitt_false(&image[..0x1e]).to_le_bytes()
        );
        assert_eq!(validate_generated_image(&image), Ok(()));
    }

    #[test]
    fn generated_header_matches_dolphin_create_id_layout() {
        let image = initialize_skylanders_placeholder(0x12_3456, Some(0x0789));

        assert_eq!(&image[0x10..0x12], &0x3456u16.to_le_bytes());
        assert_eq!(image[0x12], 0);
        assert_eq!(image[0x13], 0);
        assert_eq!(&image[0x14..0x1c], &[0; 8]);
        assert_eq!(&image[0x1c..0x1e], &0x0789u16.to_le_bytes());
        assert_eq!(
            &image[0x1e..0x20],
            &crc16_ccitt_false(&image[..0x1e]).to_le_bytes()
        );
        assert_eq!(validate_generated_image(&image), Ok(()));
    }

    #[test]
    fn mutable_character_image_initializes_and_encrypts_default_save_data() {
        let image = initialize_mutable_skylanders_entity_image(21, None, 1, FigureKind::Character);
        let plaintext = decrypt_figure(&image);

        assert_ne!(image, plaintext);
        assert_eq!(&plaintext[0x10..0x12], &21u16.to_le_bytes());
        assert_eq!(&plaintext[0x80 + 0x5a..0x80 + 0x5c], &1u16.to_le_bytes());
        assert_eq!(plaintext[0x89], 1);
        assert_eq!(plaintext[0x112], 1);
        assert_eq!(
            &plaintext[0x80 + 0x0a..0x80 + 0x0c],
            &checksum_type3(&plaintext[0x80 + 0x50..])
                .unwrap()
                .to_le_bytes()
        );
        assert_eq!(
            &plaintext[0x80 + 0x0c..0x80 + 0x0e],
            &checksum_type2(&plaintext[0x80 + 0x10..])
                .unwrap()
                .to_le_bytes()
        );
        assert_eq!(
            &plaintext[0x80 + 0x0e..0x80 + 0x10],
            &checksum_type1(&plaintext[0x80..]).unwrap().to_le_bytes()
        );
        assert_eq!(
            &plaintext[0x110..0x112],
            &checksum_type6(&plaintext[0x110..]).unwrap().to_le_bytes()
        );
        assert_has_encrypted_data_blocks(&image);
    }

    #[test]
    fn rekey_preserves_encrypted_save_data_under_new_uid() {
        let mut image =
            initialize_mutable_skylanders_entity_image(21, None, 1, FigureKind::Character);
        let before = decrypt_figure(&image);
        let old_uid = image[0..4].to_vec();

        assert!(rekey_skylanders_entity_image(&mut image, 21, None, 2));
        let after = decrypt_figure(&image);

        assert_ne!(&image[0..4], old_uid.as_slice());
        assert_eq!(&after[0x80..0x90], &before[0x80..0x90]);
        assert_eq!(&after[0x110..0x120], &before[0x110..0x120]);
        assert_eq!(&after[0x240..0x250], &before[0x240..0x250]);
        assert_eq!(&after[0x2d0..0x2e0], &before[0x2d0..0x2e0]);
        assert_eq!(&after[0x10..0x12], &21u16.to_le_bytes());
        assert_eq!(
            &after[0x1e..0x20],
            &crc16_ccitt_false(&after[..0x1e]).to_le_bytes()
        );
        assert_has_encrypted_data_blocks(&image);
    }

    fn assert_has_encrypted_data_blocks(image: &[u8; SKYLANDERS_IMAGE_BYTES]) {
        let mut encrypted_blocks = 0;
        for block_index in 8..BLOCK_COUNT {
            if block_index % 4 == 3 {
                continue;
            }
            let start = block_index * BLOCK_SIZE;
            if image[start..start + BLOCK_SIZE]
                .iter()
                .any(|byte| *byte != 0)
            {
                encrypted_blocks += 1;
            }
        }
        assert!(encrypted_blocks > 0);
    }
}
