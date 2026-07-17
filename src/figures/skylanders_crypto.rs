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
    input[0x20..0x30].copy_from_slice(data_start.get(0x30..0x40)?);
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

pub fn decrypt_figure(image: &[u8; FIGURE_SIZE]) -> [u8; FIGURE_SIZE] {
    let mut decrypted = [0; FIGURE_SIZE];
    let mut hash_input = incomplete_hash_input(image);

    for block_index in 0..BLOCK_COUNT {
        let start = block_index * BLOCK_SIZE;
        let end = start + BLOCK_SIZE;
        let mut current_block = [0; BLOCK_SIZE];
        current_block.copy_from_slice(&image[start..end]);

        if is_plaintext_block(block_index) {
            decrypted[start..end].copy_from_slice(&current_block);
            continue;
        }
        if current_block.iter().all(|byte| *byte == 0) {
            continue;
        }

        hash_input[0x20] = block_index as u8;
        let key = md5_digest(&hash_input);
        decrypted[start..end].copy_from_slice(&aes128_decrypt_block(&current_block, &key));
    }

    decrypted
}

pub fn encrypt_figure(plaintext: &[u8; FIGURE_SIZE]) -> [u8; FIGURE_SIZE] {
    let mut encrypted = [0; FIGURE_SIZE];
    let mut hash_input = incomplete_hash_input(plaintext);

    for block_index in 0..BLOCK_COUNT {
        let start = block_index * BLOCK_SIZE;
        let end = start + BLOCK_SIZE;
        let mut current_block = [0; BLOCK_SIZE];
        current_block.copy_from_slice(&plaintext[start..end]);

        if is_plaintext_block(block_index) {
            encrypted[start..end].copy_from_slice(&current_block);
            continue;
        }

        hash_input[0x20] = block_index as u8;
        let key = md5_digest(&hash_input);
        encrypted[start..end].copy_from_slice(&aes128_encrypt_block(&current_block, &key));
    }

    encrypted
}

fn incomplete_hash_input(image: &[u8; FIGURE_SIZE]) -> [u8; 0x56] {
    const HASH_CONST: &[u8; 0x35] = b" Copyright (C) 2010 Activision. All Rights Reserved. ";
    let mut hash_input = [0; 0x56];
    hash_input[0..0x10].copy_from_slice(&image[0..0x10]);
    hash_input[0x10..0x20].copy_from_slice(&image[0x10..0x20]);
    hash_input[0x21..].copy_from_slice(HASH_CONST);
    hash_input
}

fn md5_digest(input: &[u8]) -> [u8; 16] {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10,
        15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    const K: [u32; 64] = [
        0xd76a_a478,
        0xe8c7_b756,
        0x2420_70db,
        0xc1bd_ceee,
        0xf57c_0faf,
        0x4787_c62a,
        0xa830_4613,
        0xfd46_9501,
        0x6980_98d8,
        0x8b44_f7af,
        0xffff_5bb1,
        0x895c_d7be,
        0x6b90_1122,
        0xfd98_7193,
        0xa679_438e,
        0x49b4_0821,
        0xf61e_2562,
        0xc040_b340,
        0x265e_5a51,
        0xe9b6_c7aa,
        0xd62f_105d,
        0x0244_1453,
        0xd8a1_e681,
        0xe7d3_fbc8,
        0x21e1_cde6,
        0xc337_07d6,
        0xf4d5_0d87,
        0x455a_14ed,
        0xa9e3_e905,
        0xfcef_a3f8,
        0x676f_02d9,
        0x8d2a_4c8a,
        0xfffa_3942,
        0x8771_f681,
        0x6d9d_6122,
        0xfde5_380c,
        0xa4be_ea44,
        0x4bde_cfa9,
        0xf6bb_4b60,
        0xbebf_bc70,
        0x289b_7ec6,
        0xeaa1_27fa,
        0xd4ef_3085,
        0x0488_1d05,
        0xd9d4_d039,
        0xe6db_99e5,
        0x1fa2_7cf8,
        0xc4ac_5665,
        0xf429_2244,
        0x432a_ff97,
        0xab94_23a7,
        0xfc93_a039,
        0x655b_59c3,
        0x8f0c_cc92,
        0xffef_f47d,
        0x8584_5dd1,
        0x6fa8_7e4f,
        0xfe2c_e6e0,
        0xa301_4314,
        0x4e08_11a1,
        0xf753_7e82,
        0xbd3a_f235,
        0x2ad7_d2bb,
        0xeb86_d391,
    ];

    let bit_len = (input.len() as u64) * 8;
    let mut a0 = 0x6745_2301u32;
    let mut b0 = 0xefcd_ab89u32;
    let mut c0 = 0x98ba_dcfeu32;
    let mut d0 = 0x1032_5476u32;

    let mut block = [0u8; 64];
    let mut chunks = input.chunks_exact(64);
    for chunk in &mut chunks {
        block.copy_from_slice(chunk);
        md5_process_block(&block, &mut a0, &mut b0, &mut c0, &mut d0, &S, &K);
    }

    let remainder = chunks.remainder();
    block = [0; 64];
    block[..remainder.len()].copy_from_slice(remainder);
    block[remainder.len()] = 0x80;

    if remainder.len() >= 56 {
        md5_process_block(&block, &mut a0, &mut b0, &mut c0, &mut d0, &S, &K);
        block = [0; 64];
    }
    block[56..64].copy_from_slice(&bit_len.to_le_bytes());
    md5_process_block(&block, &mut a0, &mut b0, &mut c0, &mut d0, &S, &K);

    let mut out = [0; 16];
    out[0..4].copy_from_slice(&a0.to_le_bytes());
    out[4..8].copy_from_slice(&b0.to_le_bytes());
    out[8..12].copy_from_slice(&c0.to_le_bytes());
    out[12..16].copy_from_slice(&d0.to_le_bytes());
    out
}

#[allow(clippy::too_many_arguments)]
fn md5_process_block(
    block: &[u8; 64],
    a0: &mut u32,
    b0: &mut u32,
    c0: &mut u32,
    d0: &mut u32,
    s: &[u32; 64],
    k: &[u32; 64],
) {
    let mut m = [0u32; 16];
    for (index, word) in m.iter_mut().enumerate() {
        let start = index * 4;
        *word = u32::from_le_bytes(block[start..start + 4].try_into().unwrap());
    }

    let mut a = *a0;
    let mut b = *b0;
    let mut c = *c0;
    let mut d = *d0;

    for i in 0..64 {
        let (f, g) = if i < 16 {
            ((b & c) | (!b & d), i)
        } else if i < 32 {
            ((d & b) | (!d & c), (5 * i + 1) % 16)
        } else if i < 48 {
            (b ^ c ^ d, (3 * i + 5) % 16)
        } else {
            (c ^ (b | !d), (7 * i) % 16)
        };
        let next = b.wrapping_add(
            a.wrapping_add(f)
                .wrapping_add(k[i])
                .wrapping_add(m[g])
                .rotate_left(s[i]),
        );
        a = d;
        d = c;
        c = b;
        b = next;
    }

    *a0 = a0.wrapping_add(a);
    *b0 = b0.wrapping_add(b);
    *c0 = c0.wrapping_add(c);
    *d0 = d0.wrapping_add(d);
}

fn aes128_encrypt_block(block: &[u8; 16], key: &[u8; 16]) -> [u8; 16] {
    let round_keys = aes128_key_expansion(key);
    let mut state = *block;
    add_round_key(&mut state, &round_keys[0..16]);

    for round in 1..10 {
        sub_bytes(&mut state);
        shift_rows(&mut state);
        mix_columns(&mut state);
        add_round_key(&mut state, &round_keys[round * 16..(round + 1) * 16]);
    }

    sub_bytes(&mut state);
    shift_rows(&mut state);
    add_round_key(&mut state, &round_keys[160..176]);
    state
}

fn aes128_decrypt_block(block: &[u8; 16], key: &[u8; 16]) -> [u8; 16] {
    let round_keys = aes128_key_expansion(key);
    let mut state = *block;
    add_round_key(&mut state, &round_keys[160..176]);

    for round in (1..10).rev() {
        inv_shift_rows(&mut state);
        inv_sub_bytes(&mut state);
        add_round_key(&mut state, &round_keys[round * 16..(round + 1) * 16]);
        inv_mix_columns(&mut state);
    }

    inv_shift_rows(&mut state);
    inv_sub_bytes(&mut state);
    add_round_key(&mut state, &round_keys[0..16]);
    state
}

fn aes128_key_expansion(key: &[u8; 16]) -> [u8; 176] {
    const RCON: [u8; 10] = [0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80, 0x1b, 0x36];
    let mut expanded = [0; 176];
    expanded[..16].copy_from_slice(key);
    let mut bytes_generated = 16;
    let mut rcon_index = 0;
    let mut temp = [0; 4];

    while bytes_generated < 176 {
        temp.copy_from_slice(&expanded[bytes_generated - 4..bytes_generated]);
        if bytes_generated % 16 == 0 {
            temp.rotate_left(1);
            for byte in &mut temp {
                *byte = SBOX[*byte as usize];
            }
            temp[0] ^= RCON[rcon_index];
            rcon_index += 1;
        }
        for byte in temp {
            expanded[bytes_generated] = expanded[bytes_generated - 16] ^ byte;
            bytes_generated += 1;
        }
    }

    expanded
}

fn add_round_key(state: &mut [u8; 16], round_key: &[u8]) {
    for (state_byte, key_byte) in state.iter_mut().zip(round_key.iter()) {
        *state_byte ^= *key_byte;
    }
}

fn sub_bytes(state: &mut [u8; 16]) {
    for byte in state {
        *byte = SBOX[*byte as usize];
    }
}

fn inv_sub_bytes(state: &mut [u8; 16]) {
    for byte in state {
        *byte = INV_SBOX[*byte as usize];
    }
}

fn shift_rows(state: &mut [u8; 16]) {
    let old = *state;
    state[0] = old[0];
    state[4] = old[4];
    state[8] = old[8];
    state[12] = old[12];
    state[1] = old[5];
    state[5] = old[9];
    state[9] = old[13];
    state[13] = old[1];
    state[2] = old[10];
    state[6] = old[14];
    state[10] = old[2];
    state[14] = old[6];
    state[3] = old[15];
    state[7] = old[3];
    state[11] = old[7];
    state[15] = old[11];
}

fn inv_shift_rows(state: &mut [u8; 16]) {
    let old = *state;
    state[0] = old[0];
    state[4] = old[4];
    state[8] = old[8];
    state[12] = old[12];
    state[1] = old[13];
    state[5] = old[1];
    state[9] = old[5];
    state[13] = old[9];
    state[2] = old[10];
    state[6] = old[14];
    state[10] = old[2];
    state[14] = old[6];
    state[3] = old[7];
    state[7] = old[11];
    state[11] = old[15];
    state[15] = old[3];
}

fn mix_columns(state: &mut [u8; 16]) {
    for column in 0..4 {
        let start = column * 4;
        let a0 = state[start];
        let a1 = state[start + 1];
        let a2 = state[start + 2];
        let a3 = state[start + 3];
        state[start] = gf_mul(a0, 2) ^ gf_mul(a1, 3) ^ a2 ^ a3;
        state[start + 1] = a0 ^ gf_mul(a1, 2) ^ gf_mul(a2, 3) ^ a3;
        state[start + 2] = a0 ^ a1 ^ gf_mul(a2, 2) ^ gf_mul(a3, 3);
        state[start + 3] = gf_mul(a0, 3) ^ a1 ^ a2 ^ gf_mul(a3, 2);
    }
}

fn inv_mix_columns(state: &mut [u8; 16]) {
    for column in 0..4 {
        let start = column * 4;
        let a0 = state[start];
        let a1 = state[start + 1];
        let a2 = state[start + 2];
        let a3 = state[start + 3];
        state[start] = gf_mul(a0, 14) ^ gf_mul(a1, 11) ^ gf_mul(a2, 13) ^ gf_mul(a3, 9);
        state[start + 1] = gf_mul(a0, 9) ^ gf_mul(a1, 14) ^ gf_mul(a2, 11) ^ gf_mul(a3, 13);
        state[start + 2] = gf_mul(a0, 13) ^ gf_mul(a1, 9) ^ gf_mul(a2, 14) ^ gf_mul(a3, 11);
        state[start + 3] = gf_mul(a0, 11) ^ gf_mul(a1, 13) ^ gf_mul(a2, 9) ^ gf_mul(a3, 14);
    }
}

fn gf_mul(mut a: u8, mut b: u8) -> u8 {
    let mut product = 0;
    for _ in 0..8 {
        if b & 1 != 0 {
            product ^= a;
        }
        let high_bit = a & 0x80;
        a <<= 1;
        if high_bit != 0 {
            a ^= 0x1b;
        }
        b >>= 1;
    }
    product
}

const SBOX: [u8; 256] = [
    0x63, 0x7c, 0x77, 0x7b, 0xf2, 0x6b, 0x6f, 0xc5, 0x30, 0x01, 0x67, 0x2b, 0xfe, 0xd7, 0xab, 0x76,
    0xca, 0x82, 0xc9, 0x7d, 0xfa, 0x59, 0x47, 0xf0, 0xad, 0xd4, 0xa2, 0xaf, 0x9c, 0xa4, 0x72, 0xc0,
    0xb7, 0xfd, 0x93, 0x26, 0x36, 0x3f, 0xf7, 0xcc, 0x34, 0xa5, 0xe5, 0xf1, 0x71, 0xd8, 0x31, 0x15,
    0x04, 0xc7, 0x23, 0xc3, 0x18, 0x96, 0x05, 0x9a, 0x07, 0x12, 0x80, 0xe2, 0xeb, 0x27, 0xb2, 0x75,
    0x09, 0x83, 0x2c, 0x1a, 0x1b, 0x6e, 0x5a, 0xa0, 0x52, 0x3b, 0xd6, 0xb3, 0x29, 0xe3, 0x2f, 0x84,
    0x53, 0xd1, 0x00, 0xed, 0x20, 0xfc, 0xb1, 0x5b, 0x6a, 0xcb, 0xbe, 0x39, 0x4a, 0x4c, 0x58, 0xcf,
    0xd0, 0xef, 0xaa, 0xfb, 0x43, 0x4d, 0x33, 0x85, 0x45, 0xf9, 0x02, 0x7f, 0x50, 0x3c, 0x9f, 0xa8,
    0x51, 0xa3, 0x40, 0x8f, 0x92, 0x9d, 0x38, 0xf5, 0xbc, 0xb6, 0xda, 0x21, 0x10, 0xff, 0xf3, 0xd2,
    0xcd, 0x0c, 0x13, 0xec, 0x5f, 0x97, 0x44, 0x17, 0xc4, 0xa7, 0x7e, 0x3d, 0x64, 0x5d, 0x19, 0x73,
    0x60, 0x81, 0x4f, 0xdc, 0x22, 0x2a, 0x90, 0x88, 0x46, 0xee, 0xb8, 0x14, 0xde, 0x5e, 0x0b, 0xdb,
    0xe0, 0x32, 0x3a, 0x0a, 0x49, 0x06, 0x24, 0x5c, 0xc2, 0xd3, 0xac, 0x62, 0x91, 0x95, 0xe4, 0x79,
    0xe7, 0xc8, 0x37, 0x6d, 0x8d, 0xd5, 0x4e, 0xa9, 0x6c, 0x56, 0xf4, 0xea, 0x65, 0x7a, 0xae, 0x08,
    0xba, 0x78, 0x25, 0x2e, 0x1c, 0xa6, 0xb4, 0xc6, 0xe8, 0xdd, 0x74, 0x1f, 0x4b, 0xbd, 0x8b, 0x8a,
    0x70, 0x3e, 0xb5, 0x66, 0x48, 0x03, 0xf6, 0x0e, 0x61, 0x35, 0x57, 0xb9, 0x86, 0xc1, 0x1d, 0x9e,
    0xe1, 0xf8, 0x98, 0x11, 0x69, 0xd9, 0x8e, 0x94, 0x9b, 0x1e, 0x87, 0xe9, 0xce, 0x55, 0x28, 0xdf,
    0x8c, 0xa1, 0x89, 0x0d, 0xbf, 0xe6, 0x42, 0x68, 0x41, 0x99, 0x2d, 0x0f, 0xb0, 0x54, 0xbb, 0x16,
];

const INV_SBOX: [u8; 256] = [
    0x52, 0x09, 0x6a, 0xd5, 0x30, 0x36, 0xa5, 0x38, 0xbf, 0x40, 0xa3, 0x9e, 0x81, 0xf3, 0xd7, 0xfb,
    0x7c, 0xe3, 0x39, 0x82, 0x9b, 0x2f, 0xff, 0x87, 0x34, 0x8e, 0x43, 0x44, 0xc4, 0xde, 0xe9, 0xcb,
    0x54, 0x7b, 0x94, 0x32, 0xa6, 0xc2, 0x23, 0x3d, 0xee, 0x4c, 0x95, 0x0b, 0x42, 0xfa, 0xc3, 0x4e,
    0x08, 0x2e, 0xa1, 0x66, 0x28, 0xd9, 0x24, 0xb2, 0x76, 0x5b, 0xa2, 0x49, 0x6d, 0x8b, 0xd1, 0x25,
    0x72, 0xf8, 0xf6, 0x64, 0x86, 0x68, 0x98, 0x16, 0xd4, 0xa4, 0x5c, 0xcc, 0x5d, 0x65, 0xb6, 0x92,
    0x6c, 0x70, 0x48, 0x50, 0xfd, 0xed, 0xb9, 0xda, 0x5e, 0x15, 0x46, 0x57, 0xa7, 0x8d, 0x9d, 0x84,
    0x90, 0xd8, 0xab, 0x00, 0x8c, 0xbc, 0xd3, 0x0a, 0xf7, 0xe4, 0x58, 0x05, 0xb8, 0xb3, 0x45, 0x06,
    0xd0, 0x2c, 0x1e, 0x8f, 0xca, 0x3f, 0x0f, 0x02, 0xc1, 0xaf, 0xbd, 0x03, 0x01, 0x13, 0x8a, 0x6b,
    0x3a, 0x91, 0x11, 0x41, 0x4f, 0x67, 0xdc, 0xea, 0x97, 0xf2, 0xcf, 0xce, 0xf0, 0xb4, 0xe6, 0x73,
    0x96, 0xac, 0x74, 0x22, 0xe7, 0xad, 0x35, 0x85, 0xe2, 0xf9, 0x37, 0xe8, 0x1c, 0x75, 0xdf, 0x6e,
    0x47, 0xf1, 0x1a, 0x71, 0x1d, 0x29, 0xc5, 0x89, 0x6f, 0xb7, 0x62, 0x0e, 0xaa, 0x18, 0xbe, 0x1b,
    0xfc, 0x56, 0x3e, 0x4b, 0xc6, 0xd2, 0x79, 0x20, 0x9a, 0xdb, 0xc0, 0xfe, 0x78, 0xcd, 0x5a, 0xf4,
    0x1f, 0xdd, 0xa8, 0x33, 0x88, 0x07, 0xc7, 0x31, 0xb1, 0x12, 0x10, 0x59, 0x27, 0x80, 0xec, 0x5f,
    0x60, 0x51, 0x7f, 0xa9, 0x19, 0xb5, 0x4a, 0x0d, 0x2d, 0xe5, 0x7a, 0x9f, 0x93, 0xc9, 0x9c, 0xef,
    0xa0, 0xe0, 0x3b, 0x4d, 0xae, 0x2a, 0xf5, 0xb0, 0xc8, 0xeb, 0xbb, 0x3c, 0x83, 0x53, 0x99, 0x61,
    0x17, 0x2b, 0x04, 0x7e, 0xba, 0x77, 0xd6, 0x26, 0xe1, 0x69, 0x14, 0x63, 0x55, 0x21, 0x0c, 0x7d,
];

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
        assert_eq!(checksum_type3(&data), Some(0xbe5a));
        assert_eq!(checksum_type6(&data), Some(0xbcb6));
    }

    #[test]
    fn md5_matches_standard_test_vectors() {
        assert_eq!(
            md5_digest(b""),
            [
                0xd4, 0x1d, 0x8c, 0xd9, 0x8f, 0x00, 0xb2, 0x04, 0xe9, 0x80, 0x09, 0x98, 0xec, 0xf8,
                0x42, 0x7e,
            ]
        );
        assert_eq!(
            md5_digest(b"abc"),
            [
                0x90, 0x01, 0x50, 0x98, 0x3c, 0xd2, 0x4f, 0xb0, 0xd6, 0x96, 0x3f, 0x7d, 0x28, 0xe1,
                0x7f, 0x72,
            ]
        );
    }

    #[test]
    fn aes128_matches_standard_test_vector() {
        let key = [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
            0x0e, 0x0f,
        ];
        let plaintext = [
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
            0xee, 0xff,
        ];
        let ciphertext = [
            0x69, 0xc4, 0xe0, 0xd8, 0x6a, 0x7b, 0x04, 0x30, 0xd8, 0xcd, 0xb7, 0x80, 0x70, 0xb4,
            0xc5, 0x5a,
        ];

        assert_eq!(aes128_encrypt_block(&plaintext, &key), ciphertext);
        assert_eq!(aes128_decrypt_block(&ciphertext, &key), plaintext);
    }

    #[test]
    fn generated_blank_image_decrypts_to_itself() {
        let image = crate::figures::init::initialize_skylanders_entity_image(21, None, 1);

        assert_eq!(decrypt_figure(&image), image);
    }

    #[test]
    fn figure_encrypt_decrypt_round_trips_data_blocks() {
        let mut plaintext = crate::figures::init::initialize_skylanders_entity_image(21, None, 1);

        for block_index in 8..BLOCK_COUNT {
            if is_plaintext_block(block_index) {
                continue;
            }
            let start = block_index * BLOCK_SIZE;
            for byte_index in 0..BLOCK_SIZE {
                plaintext[start + byte_index] = block_index as u8 ^ byte_index as u8;
            }
        }

        let encrypted = encrypt_figure(&plaintext);
        assert_ne!(encrypted, plaintext);
        assert_eq!(decrypt_figure(&encrypted), plaintext);
    }
}
