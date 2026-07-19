use crate::figures::formats::INFINITY_IMAGE_BYTES;
use crate::figures::skylanders::crypto::{aes128_decrypt_block, aes128_encrypt_block};

const BLOCK_BYTES: usize = 16;
const FIRST_BLOCK_ACCESS: u32 = 0x17878e;
const OTHER_BLOCK_ACCESS: u32 = 0x778788;
const SHA1_KEY_PREFIX: &[u8; 31] =
    b"\xaf\x62\xd2\xec\x04\x91\x96\x8c\xc5\x2a\x1a\x71\x65\xf8\x65\xfe\x28\x63\x29 Disney 2013";
const UID_STATIC_SUFFIX: [u8; 4] = [0x89, 0x44, 0x00, 0xc2];

pub fn initialize_infinity_entity_image(
    figure_number: u32,
    entity_id: u32,
) -> [u8; INFINITY_IMAGE_BYTES] {
    let mut image = [0; INFINITY_IMAGE_BYTES];
    let uid = deterministic_uid(figure_number, entity_id);
    image[..7].copy_from_slice(&uid);
    image[7..11].copy_from_slice(&UID_STATIC_SUFFIX);

    write_access_bytes(&mut image, 0, FIRST_BLOCK_ACCESS);
    for sector in 1..5 {
        write_access_bytes(&mut image, sector, OTHER_BLOCK_ACCESS);
    }

    let figure_data = blank_figure_data(figure_number);
    let key = infinity_figure_key(uid);
    let encrypted_figure_data = aes128_encrypt_block(&figure_data, &key);
    let encrypted_blank = aes128_encrypt_block(&[0; BLOCK_BYTES], &key);

    image[0x10..0x20].copy_from_slice(&encrypted_figure_data);
    for block in [0x04usize, 0x08, 0x0c, 0x0d] {
        let offset = block * BLOCK_BYTES;
        image[offset..offset + BLOCK_BYTES].copy_from_slice(&encrypted_blank);
    }

    image
}

pub fn decrypt_infinity_figure_data(image: &[u8; INFINITY_IMAGE_BYTES]) -> [u8; BLOCK_BYTES] {
    let mut uid = [0; 7];
    uid.copy_from_slice(&image[..7]);
    let key = infinity_figure_key(uid);
    let mut encrypted = [0; BLOCK_BYTES];
    encrypted.copy_from_slice(&image[0x10..0x20]);
    aes128_decrypt_block(&encrypted, &key)
}

pub fn infinity_figure_number(image: &[u8; INFINITY_IMAGE_BYTES]) -> u32 {
    let figure_data = decrypt_infinity_figure_data(image);
    (u32::from(figure_data[1]) << 16) | (u32::from(figure_data[2]) << 8) | u32::from(figure_data[3])
}

fn write_access_bytes(image: &mut [u8; INFINITY_IMAGE_BYTES], sector: usize, access: u32) {
    let offset = sector * 0x40 + 0x36;
    image[offset] = ((access >> 16) & 0xff) as u8;
    image[offset + 1] = ((access >> 8) & 0xff) as u8;
    image[offset + 2] = (access & 0xff) as u8;
}

fn deterministic_uid(figure_number: u32, entity_id: u32) -> [u8; 7] {
    let mut state = 0x4f4d_4e49u32 ^ figure_number.rotate_left(7) ^ entity_id.rotate_left(17);
    let mut uid = [0; 7];
    for byte in &mut uid {
        state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        *byte = (state >> 24) as u8;
    }
    if uid.iter().all(|byte| *byte == 0) {
        uid[0] = 0x04;
    }
    uid
}

fn blank_figure_data(figure_number: u32) -> [u8; BLOCK_BYTES] {
    let mut data = [0; BLOCK_BYTES];
    data[1] = ((figure_number >> 16) & 0xff) as u8;
    data[2] = ((figure_number >> 8) & 0xff) as u8;
    data[3] = (figure_number & 0xff) as u8;
    data[4] = 0x0d;
    data[5] = 0x08;
    data[6] = 0x12;
    data[9] = 0x01;
    data[10] = 0xd1;
    data[11] = 0x1f;

    let checksum = infinity_crc32_12(&data);
    data[12..16].copy_from_slice(&checksum.to_be_bytes());
    data
}

fn infinity_figure_key(uid: [u8; 7]) -> [u8; BLOCK_BYTES] {
    let mut input = [0; 38];
    input[..31].copy_from_slice(SHA1_KEY_PREFIX);
    input[31..].copy_from_slice(&uid);
    let digest = sha1_digest(&input);
    let mut key = [0; BLOCK_BYTES];
    for group in 0..4 {
        for index in 0..4 {
            key[group * 4 + index] = digest[group * 4 + (3 - index)];
        }
    }
    key
}

fn infinity_crc32_12(bytes: &[u8; BLOCK_BYTES]) -> u32 {
    let mut crc = 0u32;
    for byte in bytes.iter().take(12) {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    crc
}

fn sha1_digest(input: &[u8]) -> [u8; 20] {
    let bit_len = (input.len() as u64) * 8;
    let mut h0 = 0x6745_2301u32;
    let mut h1 = 0xefcd_ab89u32;
    let mut h2 = 0x98ba_dcfeu32;
    let mut h3 = 0x1032_5476u32;
    let mut h4 = 0xc3d2_e1f0u32;

    let mut block = [0; 64];
    block[..input.len()].copy_from_slice(input);
    block[input.len()] = 0x80;
    block[56..64].copy_from_slice(&bit_len.to_be_bytes());
    sha1_process_block(&block, &mut h0, &mut h1, &mut h2, &mut h3, &mut h4);

    let mut out = [0; 20];
    out[..4].copy_from_slice(&h0.to_be_bytes());
    out[4..8].copy_from_slice(&h1.to_be_bytes());
    out[8..12].copy_from_slice(&h2.to_be_bytes());
    out[12..16].copy_from_slice(&h3.to_be_bytes());
    out[16..20].copy_from_slice(&h4.to_be_bytes());
    out
}

fn sha1_process_block(
    block: &[u8; 64],
    h0: &mut u32,
    h1: &mut u32,
    h2: &mut u32,
    h3: &mut u32,
    h4: &mut u32,
) {
    let mut w = [0u32; 80];
    for (index, chunk) in block.chunks_exact(4).enumerate() {
        w[index] = u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
    }
    for index in 16..80 {
        w[index] = (w[index - 3] ^ w[index - 8] ^ w[index - 14] ^ w[index - 16]).rotate_left(1);
    }

    let mut a = *h0;
    let mut b = *h1;
    let mut c = *h2;
    let mut d = *h3;
    let mut e = *h4;

    for (index, word) in w.iter().enumerate() {
        let (f, k) = match index {
            0..=19 => ((b & c) | ((!b) & d), 0x5a82_7999),
            20..=39 => (b ^ c ^ d, 0x6ed9_eba1),
            40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1b_bcdc),
            _ => (b ^ c ^ d, 0xca62_c1d6),
        };
        let temp = a
            .rotate_left(5)
            .wrapping_add(f)
            .wrapping_add(e)
            .wrapping_add(k)
            .wrapping_add(*word);
        e = d;
        d = c;
        c = b.rotate_left(30);
        b = a;
        a = temp;
    }

    *h0 = h0.wrapping_add(a);
    *h1 = h1.wrapping_add(b);
    *h2 = h2.wrapping_add(c);
    *h3 = h3.wrapping_add(d);
    *h4 = h4.wrapping_add(e);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::figures::infinity::find_infinity_catalog_entry;

    #[test]
    fn generated_image_decrypts_to_dolphin_blank_figure_data() {
        let image = initialize_infinity_entity_image(0x0f4241, 7);
        assert_eq!(image.len(), INFINITY_IMAGE_BYTES);
        assert_eq!(&image[7..11], &UID_STATIC_SUFFIX);
        assert_eq!(&image[0x36..0x39], &[0x17, 0x87, 0x8e]);
        assert_eq!(&image[0x76..0x79], &[0x77, 0x87, 0x88]);

        let figure_data = decrypt_infinity_figure_data(&image);
        assert_eq!(&figure_data[1..4], &[0x0f, 0x42, 0x41]);
        assert_eq!(&figure_data[4..7], &[0x0d, 0x08, 0x12]);
        assert_eq!(&figure_data[9..12], &[0x01, 0xd1, 0x1f]);
        assert_eq!(
            u32::from_be_bytes(figure_data[12..16].try_into().unwrap()),
            infinity_crc32_12(&figure_data)
        );
        assert_eq!(infinity_figure_number(&image), 0x0f4241);
        assert_eq!(
            find_infinity_catalog_entry(infinity_figure_number(&image)).map(|entry| entry.name),
            Some("Mr. Incredible")
        );
    }
}
