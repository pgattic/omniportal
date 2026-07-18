use alloc::string::String;
use alloc::vec::Vec;

use crate::domain::GameLine;

use super::StorageError;

pub(super) fn query_param(query: &str, name: &str) -> Option<String> {
    for pair in query.split('&') {
        if let Some((key, value)) = pair.split_once('=') {
            if key == name {
                return Some(percent_decode(value));
            }
        }
    }
    None
}

pub(super) fn parse_u32(value: &str) -> Option<u32> {
    if let Some(hex) = value.strip_prefix("0x") {
        u32::from_str_radix(hex, 16).ok()
    } else {
        value.parse().ok()
    }
}

pub(super) fn parse_game_param(params: &str) -> Result<Option<GameLine>, StorageError> {
    let Some(game) = query_param(params, "game") else {
        return Ok(None);
    };
    match game.as_str() {
        "" => Ok(None),
        "skylanders" => Ok(Some(GameLine::Skylanders)),
        "infinity" => Ok(Some(GameLine::Infinity)),
        _ => Err(StorageError::BadRequest),
    }
}

pub(super) fn decode_hex_bytes(value: &str) -> Option<Vec<u8>> {
    let bytes = value.as_bytes();
    if bytes.len() % 2 != 0 {
        return None;
    }
    let mut out = Vec::new();
    for pair in bytes.chunks_exact(2) {
        let high = hex_nibble(pair[0])?;
        let low = hex_nibble(pair[1])?;
        out.push(high << 4 | low);
    }
    Some(out)
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = String::new();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                out.push(' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                if let (Some(high), Some(low)) =
                    (hex_nibble(bytes[index + 1]), hex_nibble(bytes[index + 2]))
                {
                    out.push((high << 4 | low) as char);
                    index += 3;
                } else {
                    index += 1;
                }
            }
            byte => {
                out.push(byte as char);
                index += 1;
            }
        }
    }
    out
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
