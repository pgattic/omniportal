use crate::figures::formats::SKYLANDERS_IMAGE_BYTES;

pub const BLOCK_SIZE: usize = 0x10;
pub const BLOCK_COUNT: usize = 0x40;
pub const FIGURE_SIZE: usize = BLOCK_SIZE * BLOCK_COUNT;
pub const FIRST_SECTOR_TRAILER_ACL: u32 = 0x690f_0f0f;
pub const OTHER_SECTOR_TRAILER_ACL: u32 = 0x6908_0f7f;

pub fn crc16_ccitt_false(data: &[u8]) -> u16 {
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

pub fn crc48(data: &[u8]) -> u64 {
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

pub fn calculate_key_a(sector: u8, nuid: [u8; 4]) -> u64 {
    if sector == 0 {
        return 73 * 2017 * 560_381_651;
    }

    let crc = crc48(&[nuid[0], nuid[1], nuid[2], nuid[3], sector]);
    crc.swap_bytes() >> 16
}

pub fn checksum_type0(data_start: &[u8]) -> Option<u16> {
    Some(crc16_ccitt_false(data_start.get(..0x1e)?))
}

pub fn checksum_type1(data_start: &[u8]) -> Option<u16> {
    let source = data_start.get(..0x10)?;
    let mut input = [0; 0x10];
    input.copy_from_slice(source);
    input[0x0e] = 0x05;
    input[0x0f] = 0x00;
    Some(crc16_ccitt_false(&input))
}

pub fn checksum_type2(data_start: &[u8]) -> Option<u16> {
    let mut input = [0; 0x30];
    input[..0x20].copy_from_slice(data_start.get(..0x20)?);
    input[0x20..].copy_from_slice(data_start.get(0x30..0x40)?);
    Some(crc16_ccitt_false(&input))
}

pub fn checksum_type3(data_start: &[u8]) -> Option<u16> {
    let mut input = [0; 0x110];
    input[..0x20].copy_from_slice(data_start.get(..0x20)?);
    input[0x20..0x110].copy_from_slice(data_start.get(0x30..0x120)?);
    Some(crc16_ccitt_false(&input))
}

pub fn checksum_type6(data_start: &[u8]) -> Option<u16> {
    let mut input = [0; 0x40];
    input[..0x20].copy_from_slice(data_start.get(..0x20)?);
    input[0x20..].copy_from_slice(data_start.get(0x30..0x50)?);
    input[0] = 0x06;
    input[1] = 0x01;
    Some(crc16_ccitt_false(&input))
}

pub fn is_sector_trailer_block(block: usize) -> bool {
    block % 4 == 3
}

pub fn is_plaintext_block(block: usize) -> bool {
    block < 8 || is_sector_trailer_block(block)
}

pub fn block(image: &[u8], block: usize) -> Option<&[u8]> {
    let start = block.checked_mul(BLOCK_SIZE)?;
    image.get(start..start + BLOCK_SIZE)
}

pub fn validate_generated_image(image: &[u8]) -> Result<(), GeneratedImageError> {
    if image.len() != SKYLANDERS_IMAGE_BYTES || image.len() != FIGURE_SIZE {
        return Err(GeneratedImageError::Length);
    }
    if image[4] != image[0] ^ image[1] ^ image[2] ^ image[3] {
        return Err(GeneratedImageError::Bcc);
    }
    if image[5..8] != [0x81, 0x01, 0x0f] {
        return Err(GeneratedImageError::TagType);
    }
    let expected_header_crc = checksum_type0(image).ok_or(GeneratedImageError::HeaderChecksum)?;
    if image[0x1e..0x20] != expected_header_crc.to_le_bytes() {
        return Err(GeneratedImageError::HeaderChecksum);
    }
    validate_sector_trailers(image)?;
    validate_sector_keys(image)?;
    validate_generated_blank_ciphertext(image)?;
    Ok(())
}

fn validate_sector_trailers(image: &[u8]) -> Result<(), GeneratedImageError> {
    if image[0x36..0x3a] != FIRST_SECTOR_TRAILER_ACL.to_le_bytes() {
        return Err(GeneratedImageError::SectorTrailer(0));
    }
    for sector in 1..0x10 {
        let offset = sector * 0x40 + 0x36;
        if image[offset..offset + 4] != OTHER_SECTOR_TRAILER_ACL.to_le_bytes() {
            return Err(GeneratedImageError::SectorTrailer(sector as u8));
        }
    }
    Ok(())
}

fn validate_sector_keys(image: &[u8]) -> Result<(), GeneratedImageError> {
    let nuid = [image[0], image[1], image[2], image[3]];
    for sector in 0..0x10 {
        let key = calculate_key_a(sector as u8, nuid);
        let offset = sector * 0x40 + 0x30;
        for index in 0..6 {
            if image[offset + index] != (key >> ((5 - index) * 8)) as u8 {
                return Err(GeneratedImageError::SectorKey(sector as u8));
            }
        }
    }
    Ok(())
}

fn validate_generated_blank_ciphertext(image: &[u8]) -> Result<(), GeneratedImageError> {
    for block_index in 8..BLOCK_COUNT {
        if is_plaintext_block(block_index) {
            continue;
        }
        let block = block(image, block_index).ok_or(GeneratedImageError::Length)?;
        if block.iter().any(|byte| *byte != 0) {
            return Err(GeneratedImageError::EncryptedDataBlock(block_index as u8));
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GeneratedImageError {
    Length,
    Bcc,
    TagType,
    HeaderChecksum,
    SectorTrailer(u8),
    SectorKey(u8),
    EncryptedDataBlock(u8),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksums_match_dolphin_reference_algorithms() {
        assert_eq!(crc16_ccitt_false(b""), 0xffff);
        assert_eq!(crc16_ccitt_false(b"123456789"), 0x29b1);
        assert_eq!(crc48(&[0x4f, 0x4d, 0x4e, 0x49, 1]), 0x55c6_1cfa_4e83);
        assert_eq!(calculate_key_a(0, *b"OMNI"), 0x4b0b_2010_7ccb);
        assert_eq!(calculate_key_a(1, *b"OMNI"), 0x834e_fa1c_c655);
    }

    #[test]
    fn checksum_variants_are_deterministic() {
        let mut data = [0u8; 0x120];
        for (index, byte) in data.iter_mut().enumerate() {
            *byte = index as u8;
        }

        assert_eq!(checksum_type0(&data), Some(0x3554));
        assert_eq!(checksum_type1(&data), Some(0x1622));
        assert_eq!(checksum_type2(&data), Some(0x543e));
        assert_eq!(checksum_type3(&data), Some(0xed5f));
        assert_eq!(checksum_type6(&data), Some(0xbcb6));
    }
}
