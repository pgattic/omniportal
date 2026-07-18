use alloc::format;
use alloc::string::String;

use crate::storage::records::{BlobId, RecordId};

pub(super) fn json_escape(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out
}

pub(super) fn option_u32_json(value: Option<u32>) -> String {
    value
        .map(|value| format!("{}", value))
        .unwrap_or_else(|| String::from("null"))
}

pub(super) fn option_u16_json(value: Option<u16>) -> String {
    value
        .map(|value| format!("{}", value))
        .unwrap_or_else(|| String::from("null"))
}

pub(super) fn option_record_id_json(value: Option<RecordId>) -> String {
    value
        .map(|value| format!("{}", value.0))
        .unwrap_or_else(|| String::from("null"))
}

pub(super) fn option_blob_id_json(value: Option<BlobId>) -> String {
    value
        .map(|value| format!("{}", value.0))
        .unwrap_or_else(|| String::from("null"))
}

pub(super) fn option_str_json(value: Option<&str>) -> String {
    value
        .map(|value| format!("\"{}\"", json_escape(value)))
        .unwrap_or_else(|| String::from("null"))
}

pub(super) fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::new();
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}
