use alloc::vec::Vec;

use crate::config;
use crate::domain::{FigureKind, GameLine, ImageFormat};
use crate::storage::catalog::Catalog;
use crate::storage::records::{
    BlobId, CharacterIdentity, Entity, EntityDataMode, FixedText, RecordId, StoredBlob,
    MAX_RECORD_NAME_BYTES,
};
use crate::storage::wear::{JOURNAL_RECORD_HEADER_BYTES, JOURNAL_RECORD_MAGIC};
use crate::usb::skylanders::MAX_FIGURES;

use super::{StorageError, StorageFlash};

pub(super) const RECORD_KIND_IDENTITY_UPSERT: u8 = 1;
pub(super) const RECORD_KIND_IDENTITY_DELETE: u8 = 2;
pub(super) const RECORD_KIND_ENTITY_UPSERT: u8 = 3;
pub(super) const RECORD_KIND_ENTITY_DELETE: u8 = 4;
pub(super) const RECORD_KIND_BLOB_DATA: u8 = 7;
pub(super) const RECORD_KIND_CONFIG_UPSERT: u8 = 8;
pub(super) const RECORD_KIND_FORMAT_MARKER: u8 = 254;

const ERASED_WORD: [u8; 4] = [0xff; 4];
const CONFIG_SLOTS_MAGIC: &[u8; 4] = b"SLT1";

pub(super) fn scan_flash(
    flash: &mut StorageFlash,
    catalog: &mut Catalog,
) -> Result<(), StorageError> {
    let mut offset = 0;
    let mut applied_records = 0;
    let mut word = [0; 4];
    while offset + JOURNAL_RECORD_HEADER_BYTES as u32 <= config::STORAGE_FLASH_BYTES {
        flash
            .read(config::STORAGE_FLASH_OFFSET + offset, &mut word)
            .map_err(|_| StorageError::Flash)?;
        if word == ERASED_WORD {
            catalog.write_offset = offset;
            return Ok(());
        }
        if u32::from_le_bytes(word) != JOURNAL_RECORD_MAGIC {
            catalog.corrupt_records += 1;
            return stop_at_corrupt_tail(catalog, offset, applied_records);
        }

        let mut header = [0; JOURNAL_RECORD_HEADER_BYTES];
        flash
            .read(config::STORAGE_FLASH_OFFSET + offset, &mut header)
            .map_err(|_| StorageError::Flash)?;
        let Some(record) = JournalHeader::decode(&header) else {
            catalog.corrupt_records += 1;
            return stop_at_corrupt_tail(catalog, offset, applied_records);
        };
        let total_len = align4(JOURNAL_RECORD_HEADER_BYTES as u32 + record.payload_len);
        if offset + total_len > config::STORAGE_FLASH_BYTES {
            catalog.corrupt_records += 1;
            return stop_at_corrupt_tail(catalog, offset, applied_records);
        }

        let payload_offset = offset + JOURNAL_RECORD_HEADER_BYTES as u32;
        let mut payload = Vec::new();
        let padded_payload_len = total_len - JOURNAL_RECORD_HEADER_BYTES as u32;
        payload.resize(padded_payload_len as usize, 0);
        if padded_payload_len > 0 {
            flash
                .read(config::STORAGE_FLASH_OFFSET + payload_offset, &mut payload)
                .map_err(|_| StorageError::Flash)?;
        }
        payload.truncate(record.payload_len as usize);
        if crc32(&payload) != record.payload_crc {
            catalog.corrupt_records += 1;
            return stop_at_corrupt_tail(catalog, offset, applied_records);
        }

        apply_record(catalog, &record, payload_offset, &payload)?;
        applied_records += 1;
        offset += total_len;
        catalog.write_offset = offset;
    }
    Ok(())
}

fn stop_at_corrupt_tail(
    catalog: &mut Catalog,
    offset: u32,
    applied_records: u32,
) -> Result<(), StorageError> {
    if applied_records == 0 {
        catalog.write_offset = offset;
        return Err(StorageError::Corrupt);
    }

    catalog.write_offset = next_sector_offset(offset);
    Ok(())
}

fn next_sector_offset(offset: u32) -> u32 {
    const SECTOR_BYTES: u32 = 4096;
    let next = (offset + SECTOR_BYTES) & !(SECTOR_BYTES - 1);
    next.min(config::STORAGE_FLASH_BYTES)
}

fn apply_record(
    catalog: &mut Catalog,
    record: &JournalHeader,
    payload_offset: u32,
    payload: &[u8],
) -> Result<(), StorageError> {
    match record.kind {
        RECORD_KIND_IDENTITY_UPSERT => {
            if let Some(identity) = decode_identity(record.id, record.generation, payload) {
                catalog.observe_record_id(record.id, record.generation);
                catalog.upsert_identity(identity)?;
            }
        }
        RECORD_KIND_IDENTITY_DELETE => {
            catalog.observe_record_id(record.id, record.generation);
            let _ = catalog.delete_identity(RecordId(record.id));
        }
        RECORD_KIND_ENTITY_UPSERT => {
            if let Some(entity) = decode_entity(record.id, record.generation, payload) {
                catalog.observe_record_id(record.id, record.generation);
                catalog.upsert_entity(entity)?;
            }
        }
        RECORD_KIND_ENTITY_DELETE => {
            catalog.observe_record_id(record.id, record.generation);
            let _ = catalog.delete_entity(RecordId(record.id));
        }
        RECORD_KIND_BLOB_DATA => {
            catalog.observe_blob_id(record.id, record.generation);
            catalog.upsert_blob(StoredBlob {
                id: BlobId(record.id),
                offset: payload_offset,
                len: record.payload_len,
                crc32: record.payload_crc,
                generation: record.generation,
            })?;
        }
        RECORD_KIND_FORMAT_MARKER => {
            catalog.next_generation = catalog.next_generation.max(record.generation + 1);
        }
        RECORD_KIND_CONFIG_UPSERT => {
            catalog.next_generation = catalog.next_generation.max(record.generation + 1);
            let config = decode_config(payload);
            catalog.active_slots = config.active_slots;
            catalog.usb_mode = config.usb_mode;
            catalog.active_config_generation = record.generation;
        }
        _ => {}
    }
    Ok(())
}
pub(super) fn append_record(
    flash: &mut StorageFlash,
    catalog: &mut Catalog,
    kind: u8,
    id: u32,
    generation: u32,
    payload: &[u8],
) -> Result<(), StorageError> {
    if catalog.needs_format {
        return Err(StorageError::NeedsFormat);
    }

    let payload_len = payload.len() as u32;
    let total_len = align4(JOURNAL_RECORD_HEADER_BYTES as u32 + payload_len);
    if catalog.write_offset + total_len > config::STORAGE_FLASH_BYTES {
        return Err(StorageError::Full);
    }

    let header = JournalHeader {
        kind,
        id,
        generation,
        payload_len,
        payload_crc: crc32(payload),
    };
    let mut record = Vec::new();
    record.extend_from_slice(&header.encode());
    record.extend_from_slice(payload);
    while record.len() % 4 != 0 {
        record.push(0xff);
    }

    flash
        .write(config::STORAGE_FLASH_OFFSET + catalog.write_offset, &record)
        .map_err(|_| StorageError::Flash)?;
    verify_appended_record(flash, catalog.write_offset, &record)?;
    catalog.write_offset += record.len() as u32;
    Ok(())
}

fn verify_appended_record(
    flash: &mut StorageFlash,
    offset: u32,
    expected: &[u8],
) -> Result<(), StorageError> {
    let mut actual = Vec::new();
    actual.resize(expected.len(), 0);
    flash
        .read(config::STORAGE_FLASH_OFFSET + offset, &mut actual)
        .map_err(|_| StorageError::Flash)?;
    if actual == expected {
        Ok(())
    } else {
        Err(StorageError::Flash)
    }
}

pub(super) struct JournalHeader {
    pub(super) kind: u8,
    pub(super) id: u32,
    pub(super) generation: u32,
    pub(super) payload_len: u32,
    pub(super) payload_crc: u32,
}

impl JournalHeader {
    pub(super) fn encode(&self) -> [u8; JOURNAL_RECORD_HEADER_BYTES] {
        let mut out = [0; JOURNAL_RECORD_HEADER_BYTES];
        out[0..4].copy_from_slice(&JOURNAL_RECORD_MAGIC.to_le_bytes());
        out[4] = self.kind;
        out[5] = 1;
        out[6..8].copy_from_slice(&(JOURNAL_RECORD_HEADER_BYTES as u16).to_le_bytes());
        out[8..12].copy_from_slice(&self.payload_len.to_le_bytes());
        out[12..16].copy_from_slice(&self.id.to_le_bytes());
        out[16..20].copy_from_slice(&self.generation.to_le_bytes());
        out[20..24].copy_from_slice(&self.payload_crc.to_le_bytes());
        out
    }

    pub(super) fn decode(bytes: &[u8; JOURNAL_RECORD_HEADER_BYTES]) -> Option<Self> {
        if u32::from_le_bytes(bytes[0..4].try_into().ok()?) != JOURNAL_RECORD_MAGIC {
            return None;
        }
        if bytes[5] != 1 {
            return None;
        }
        Some(Self {
            kind: bytes[4],
            payload_len: u32::from_le_bytes(bytes[8..12].try_into().ok()?),
            id: u32::from_le_bytes(bytes[12..16].try_into().ok()?),
            generation: u32::from_le_bytes(bytes[16..20].try_into().ok()?),
            payload_crc: u32::from_le_bytes(bytes[20..24].try_into().ok()?),
        })
    }
}

pub(super) fn encode_identity(identity: &CharacterIdentity) -> [u8; 176] {
    let mut out = [0; 176];
    out[0] = identity.game_line.as_u8();
    out[1] = identity.kind.as_u8();
    out[2] = identity.image_format.as_u8();
    out[3] = u8::from(identity.variant_id.is_some());
    out[4..8].copy_from_slice(&identity.character_id.to_le_bytes());
    out[8..12].copy_from_slice(&identity.variant_id.unwrap_or(0).to_le_bytes());
    out[12..16].copy_from_slice(&identity.checksum.to_le_bytes());
    out[16] = identity.name.len() as u8;
    out[17] = identity.source_notes.len() as u8;
    out[20..20 + identity.name.len()].copy_from_slice(identity.name.raw_bytes());
    let source_start = 20 + MAX_RECORD_NAME_BYTES;
    out[source_start..source_start + identity.source_notes.len()]
        .copy_from_slice(identity.source_notes.raw_bytes());
    out
}

pub(super) fn decode_identity(
    id: u32,
    generation: u32,
    payload: &[u8],
) -> Option<CharacterIdentity> {
    if payload.len() < 176 {
        return None;
    }
    let name_len = payload[16] as usize;
    let source_len = payload[17] as usize;
    let source_start = 20 + MAX_RECORD_NAME_BYTES;
    if name_len > MAX_RECORD_NAME_BYTES
        || source_len > crate::storage::records::MAX_SOURCE_NOTES_BYTES
        || 20 + name_len > source_start
        || source_start + source_len > payload.len()
    {
        return None;
    }
    let name = core::str::from_utf8(&payload[20..20 + name_len]).ok()?;
    let source = if source_len == 0 {
        FixedText::empty()
    } else {
        FixedText::from_str(
            core::str::from_utf8(&payload[source_start..source_start + source_len]).ok()?,
        )
        .ok()?
    };
    Some(CharacterIdentity {
        id: RecordId(id),
        game_line: GameLine::from_u8(payload[0])?,
        kind: FigureKind::from_u8(payload[1]),
        image_format: ImageFormat::from_u8(payload[2])?,
        variant_id: if payload[3] == 1 {
            Some(u32::from_le_bytes(payload[8..12].try_into().ok()?))
        } else {
            None
        },
        character_id: u32::from_le_bytes(payload[4..8].try_into().ok()?),
        checksum: u32::from_le_bytes(payload[12..16].try_into().ok()?),
        name: FixedText::from_str(name).ok()?,
        source_notes: source,
        generation,
    })
}

pub(super) fn encode_entity(entity: &Entity) -> [u8; 128] {
    let mut out = [0; 128];
    out[0] = entity.game_line.as_u8();
    out[1] = entity.image_format.as_u8();
    out[2] = entity.name.len() as u8;
    out[3] = u8::from(entity.parent_identity_id.is_some());
    out[4] = entity.data_mode.as_u8();
    out[5] = entity.kind.as_u8();
    out[6] = u8::from(entity.catalog_index.is_some());
    out[7] = u8::from(entity.variant_id.is_some());
    out[8..12].copy_from_slice(
        &entity
            .parent_identity_id
            .map(|id| id.0)
            .unwrap_or(0)
            .to_le_bytes(),
    );
    out[12..16].copy_from_slice(&entity.blob_id.map(|id| id.0).unwrap_or(0).to_le_bytes());
    out[16..20].copy_from_slice(&entity.image_len.to_le_bytes());
    out[20..24].copy_from_slice(&entity.image_crc32.to_le_bytes());
    out[24..28].copy_from_slice(&entity.created_generation.to_le_bytes());
    out[28..32].copy_from_slice(&entity.updated_generation.to_le_bytes());
    out[32..36].copy_from_slice(&entity.character_id.to_le_bytes());
    out[36..40].copy_from_slice(&entity.variant_id.unwrap_or(0).to_le_bytes());
    out[40..42].copy_from_slice(&entity.catalog_index.unwrap_or(0).to_le_bytes());
    out[48..48 + entity.name.len()].copy_from_slice(entity.name.raw_bytes());
    out
}

pub(super) fn decode_entity(id: u32, _generation: u32, payload: &[u8]) -> Option<Entity> {
    if payload.len() < 128 {
        return None;
    }
    let name_len = payload[2] as usize;
    if name_len > MAX_RECORD_NAME_BYTES || 48 + name_len > payload.len() {
        return None;
    }
    let name = core::str::from_utf8(&payload[48..48 + name_len]).ok()?;
    Some(Entity {
        id: RecordId(id),
        game_line: GameLine::from_u8(payload[0])?,
        image_format: ImageFormat::from_u8(payload[1])?,
        name: FixedText::from_str(name).ok()?,
        parent_identity_id: if payload[3] == 1 {
            Some(RecordId(u32::from_le_bytes(
                payload[8..12].try_into().ok()?,
            )))
        } else {
            None
        },
        catalog_index: if payload[6] == 1 {
            Some(u16::from_le_bytes(payload[40..42].try_into().ok()?))
        } else {
            None
        },
        kind: FigureKind::from_u8(payload[5]),
        data_mode: EntityDataMode::from_u8(payload[4])?,
        character_id: u32::from_le_bytes(payload[32..36].try_into().ok()?),
        variant_id: if payload[7] == 1 {
            Some(u32::from_le_bytes(payload[36..40].try_into().ok()?))
        } else {
            None
        },
        blob_id: if payload[4] == EntityDataMode::MutableImage.as_u8() {
            Some(BlobId(u32::from_le_bytes(payload[12..16].try_into().ok()?)))
        } else {
            None
        },
        image_len: u32::from_le_bytes(payload[16..20].try_into().ok()?),
        image_crc32: u32::from_le_bytes(payload[20..24].try_into().ok()?),
        created_generation: u32::from_le_bytes(payload[24..28].try_into().ok()?),
        updated_generation: u32::from_le_bytes(payload[28..32].try_into().ok()?),
    })
}

pub(super) fn encode_config(
    active_slots: [Option<RecordId>; MAX_FIGURES],
    usb_mode: GameLine,
) -> [u8; 8 + MAX_FIGURES * 4] {
    let mut out = [0; 8 + MAX_FIGURES * 4];
    out[0..4].copy_from_slice(CONFIG_SLOTS_MAGIC);
    out[4] = usb_mode.as_u8();
    for (slot, entity_id) in active_slots.iter().enumerate() {
        let start = 8 + slot * 4;
        out[start..start + 4].copy_from_slice(&entity_id.map(|id| id.0).unwrap_or(0).to_le_bytes());
    }
    out
}

#[derive(Clone, Copy)]
pub(super) struct DecodedConfig {
    pub(super) active_slots: [Option<RecordId>; MAX_FIGURES],
    pub(super) usb_mode: GameLine,
}

pub(super) fn decode_config(payload: &[u8]) -> DecodedConfig {
    let mut active_slots = [None; MAX_FIGURES];
    let mut usb_mode = GameLine::Skylanders;

    if payload.len() >= 8 + MAX_FIGURES * 4 && &payload[0..4] == CONFIG_SLOTS_MAGIC {
        usb_mode = GameLine::from_u8(payload[4]).unwrap_or(GameLine::Skylanders);
        for (slot, active_slot) in active_slots.iter_mut().enumerate() {
            let start = 8 + slot * 4;
            let id = u32::from_le_bytes(payload[start..start + 4].try_into().unwrap_or([0; 4]));
            if id != 0 {
                *active_slot = Some(RecordId(id));
            }
        }
        return DecodedConfig {
            active_slots,
            usb_mode,
        };
    }

    if payload.len() >= 4 + MAX_FIGURES * 4 && &payload[0..4] == CONFIG_SLOTS_MAGIC {
        for (slot, active_slot) in active_slots.iter_mut().enumerate() {
            let start = 4 + slot * 4;
            let id = u32::from_le_bytes(payload[start..start + 4].try_into().unwrap_or([0; 4]));
            if id != 0 {
                *active_slot = Some(RecordId(id));
            }
        }
        return DecodedConfig {
            active_slots,
            usb_mode,
        };
    }

    if payload.len() >= 8 && payload[0] != 0 {
        if let Ok(bytes) = payload[4..8].try_into() {
            let id = u32::from_le_bytes(bytes);
            if id != 0 {
                active_slots[0] = Some(RecordId(id));
            }
        }
    }
    DecodedConfig {
        active_slots,
        usb_mode,
    }
}

pub(super) fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff;
    for byte in bytes {
        crc ^= *byte as u32;
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

pub(super) fn align4(value: u32) -> u32 {
    (value + 3) & !3
}
