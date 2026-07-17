pub const DEFAULT_ENTITY_NAME: &str = "Fresh Entity";

use crate::figures::formats::SKYLANDERS_IMAGE_BYTES;

pub fn initialize_skylanders_placeholder(
    character_id: u32,
    variant_id: Option<u32>,
) -> [u8; SKYLANDERS_IMAGE_BYTES] {
    let mut image = [0; SKYLANDERS_IMAGE_BYTES];
    image[0..4].copy_from_slice(b"OMNI");
    image[4] = image[0] ^ image[1] ^ image[2] ^ image[3];
    image[5] = 0x81;
    image[6] = 0x01;
    image[7] = 0x0f;

    populate_sector_trailers(&mut image);

    image[0x10..0x12].copy_from_slice(&(character_id as u16).to_le_bytes());
    image[0x1c..0x1e].copy_from_slice(&(variant_id.unwrap_or(0) as u16).to_le_bytes());
    compute_checksum_type0(&mut image);
    populate_keys(&mut image);
    image
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
        assert_eq!(&image[0x1c..0x1e], &0u16.to_le_bytes());
        assert_eq!(&image[0x1e..0x20], &crc16(&image[..0x1e]).to_le_bytes());
        assert_eq!(&image[0x36..0x3a], &0x690f_0f0fu32.to_le_bytes());
        assert_eq!(&image[0x76..0x7a], &0x6908_0f7fu32.to_le_bytes());
    }
}
