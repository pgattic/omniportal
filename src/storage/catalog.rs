use alloc::format;
use alloc::string::String;

use crate::config;
use crate::domain::GameLine;
use crate::figures::infinity::infinity_catalog_entry;
use crate::figures::skylanders::catalog::skylanders_catalog_entry;
use crate::storage::records::{
    BlobId, CharacterIdentity, Entity, RecordId, StoredBlob, MAX_ENTITIES, MAX_IDENTITIES,
};
use crate::usb::skylanders::MAX_FIGURES;

use super::json::{
    json_escape, option_blob_id_json, option_record_id_json, option_str_json, option_u16_json,
    option_u32_json,
};
use super::StorageError;

#[derive(Clone, Copy)]
pub(super) struct Catalog {
    pub(super) identities: [Option<CharacterIdentity>; MAX_IDENTITIES],
    pub(super) entities: [Option<Entity>; MAX_ENTITIES],
    pub(super) blobs: [Option<StoredBlob>; MAX_ENTITIES],
    pub(super) usb_mode: GameLine,
    pub(super) active_slots: [Option<RecordId>; MAX_FIGURES],
    pub(super) active_config_generation: u32,
    pub(super) needs_format: bool,
    pub(super) write_offset: u32,
    pub(super) next_record_id: u32,
    pub(super) next_blob_id: u32,
    pub(super) next_generation: u32,
    pub(super) corrupt_records: u32,
}

impl Catalog {
    pub(super) const fn new() -> Self {
        Self {
            identities: [None; MAX_IDENTITIES],
            entities: [None; MAX_ENTITIES],
            blobs: [None; MAX_ENTITIES],
            usb_mode: GameLine::Skylanders,
            active_slots: [None; MAX_FIGURES],
            active_config_generation: 0,
            needs_format: false,
            write_offset: 0,
            next_record_id: 1,
            next_blob_id: 1,
            next_generation: 1,
            corrupt_records: 0,
        }
    }

    pub(super) fn next_record_id(&mut self) -> RecordId {
        let id = self.next_record_id;
        self.next_record_id += 1;
        RecordId(id)
    }

    pub(super) fn next_blob_id(&mut self) -> BlobId {
        let id = self.next_blob_id;
        self.next_blob_id += 1;
        BlobId(id)
    }

    pub(super) fn next_generation(&mut self) -> u32 {
        let generation = self.next_generation;
        self.next_generation += 1;
        generation
    }

    pub(super) fn observe_record_id(&mut self, id: u32, generation: u32) {
        self.next_record_id = self.next_record_id.max(id.saturating_add(1));
        self.next_generation = self.next_generation.max(generation.saturating_add(1));
    }

    pub(super) fn observe_blob_id(&mut self, id: u32, generation: u32) {
        self.next_blob_id = self.next_blob_id.max(id.saturating_add(1));
        self.next_generation = self.next_generation.max(generation.saturating_add(1));
    }

    pub(super) fn identity(&self, id: RecordId) -> Option<CharacterIdentity> {
        self.identities
            .iter()
            .flatten()
            .find(|item| item.id == id)
            .copied()
    }

    pub(super) fn entity(&self, id: RecordId) -> Option<Entity> {
        self.entities
            .iter()
            .flatten()
            .find(|item| item.id == id)
            .copied()
    }

    pub(super) fn blob(&self, id: BlobId) -> Option<StoredBlob> {
        self.blobs
            .iter()
            .flatten()
            .find(|item| item.id == id)
            .copied()
    }

    pub(super) fn upsert_identity(
        &mut self,
        identity: CharacterIdentity,
    ) -> Result<(), StorageError> {
        upsert_by_id(&mut self.identities, identity, |item| item.id, identity.id)
    }

    pub(super) fn upsert_entity(&mut self, entity: Entity) -> Result<(), StorageError> {
        upsert_by_id(&mut self.entities, entity, |item| item.id, entity.id)
    }

    pub(super) fn upsert_blob(&mut self, blob: StoredBlob) -> Result<(), StorageError> {
        upsert_by_id(&mut self.blobs, blob, |item| item.id, blob.id)
    }

    pub(super) fn delete_identity(&mut self, id: RecordId) -> Result<(), StorageError> {
        delete_by_id(&mut self.identities, |item| item.id, id)
    }

    pub(super) fn delete_entity(&mut self, id: RecordId) -> Result<(), StorageError> {
        for slot in &mut self.active_slots {
            if *slot == Some(id) {
                *slot = None;
            }
        }
        delete_by_id(&mut self.entities, |item| item.id, id)
    }

    pub(super) fn active_entity_id(&self) -> Option<RecordId> {
        self.active_slots.iter().find_map(|id| *id)
    }

    pub(super) fn active_slots_json(&self) -> String {
        let mut out = String::from("[");
        let mut first = true;
        for (slot, entity_id) in self.active_slots.iter().enumerate() {
            let Some(entity_id) = entity_id else {
                continue;
            };
            if !first {
                out.push(',');
            }
            first = false;
            out.push_str(&format!(
                "{{\"slot\":{},\"entity_id\":{}}}",
                slot, entity_id.0
            ));
        }
        out.push(']');
        out
    }

    pub(super) fn place_entity_in_slot(&mut self, id: RecordId, slot: usize) {
        for active_slot in &mut self.active_slots {
            if *active_slot == Some(id) {
                *active_slot = None;
            }
        }
        if let Some(active_slot) = self.active_slots.get_mut(slot) {
            *active_slot = Some(id);
        }
    }

    pub(super) fn clear_transient_active_slots(&mut self) {
        self.active_slots = [None; MAX_FIGURES];
    }

    pub(super) fn identity_count(&self) -> usize {
        self.identities.iter().filter(|item| item.is_some()).count()
    }

    pub(super) fn entity_count(&self) -> usize {
        self.entities.iter().filter(|item| item.is_some()).count()
    }

    pub(super) fn status_json(&self) -> String {
        format!(
            "{{\"storage\":\"{}\",\"usb_mode\":\"{}\",\"identities\":{},\"entities\":{},\"active_entity_id\":{},\"active_slots\":{},\"used_bytes\":{},\"capacity_bytes\":{},\"corrupt_records\":{}}}",
            if self.needs_format {
                "needs-format"
            } else {
                "ok"
            },
            self.usb_mode.wire_name(),
            self.identity_count(),
            self.entity_count(),
            option_record_id_json(self.active_entity_id()),
            self.active_slots_json(),
            self.write_offset,
            config::STORAGE_FLASH_BYTES,
            self.corrupt_records
        )
    }

    pub(super) fn library_json(&self) -> String {
        let mut out = String::from("{\"identities\":[");
        let mut first = true;
        for identity in self.identities.iter().flatten() {
            if !first {
                out.push(',');
            }
            first = false;
            out.push_str(&format!(
                "{{\"id\":{},\"game\":\"{}\",\"name\":\"{}\",\"character_id\":{},\"variant_id\":{},\"kind\":\"{}\",\"format\":\"{}\"}}",
                identity.id.0,
                identity.game_line.wire_name(),
                json_escape(identity.name.as_str()),
                identity.character_id,
                option_u32_json(identity.variant_id),
                identity.kind.wire_name(),
                identity.image_format.wire_name()
            ));
        }
        out.push_str("],\"entities\":[");
        first = true;
        for entity in self.entities.iter().flatten() {
            if !first {
                out.push(',');
            }
            first = false;
            let figure_name = entity
                .catalog_index
                .and_then(|index| match entity.game_line {
                    GameLine::Skylanders => skylanders_catalog_entry(index).map(|entry| entry.name),
                    GameLine::Infinity => infinity_catalog_entry(index).map(|entry| entry.name),
                });
            out.push_str(&format!(
                "{{\"id\":{},\"name\":\"{}\",\"figure\":{},\"identity_id\":{},\"catalog_index\":{},\"game\":\"{}\",\"kind\":\"{}\",\"data_mode\":\"{}\",\"character_id\":{},\"variant_id\":{},\"blob_id\":{},\"image_len\":{},\"crc32\":{}}}",
                entity.id.0,
                json_escape(entity.name.as_str()),
                option_str_json(figure_name),
                option_record_id_json(entity.parent_identity_id),
                option_u16_json(entity.catalog_index),
                entity.game_line.wire_name(),
                entity.kind.wire_name(),
                entity.data_mode.wire_name(),
                entity.character_id,
                option_u32_json(entity.variant_id),
                option_blob_id_json(entity.blob_id),
                entity.image_len,
                entity.image_crc32
            ));
        }
        out.push_str("],\"active_entity_id\":");
        out.push_str(&option_record_id_json(self.active_entity_id()));
        out.push_str(",\"active_slots\":");
        out.push_str(&self.active_slots_json());
        out.push_str("}\n");
        out
    }
}

fn upsert_by_id<T: Copy, I: Eq>(
    items: &mut [Option<T>],
    value: T,
    id_of: impl Fn(T) -> I,
    id: I,
) -> Result<(), StorageError> {
    if let Some(slot) = items
        .iter_mut()
        .find(|slot| slot.map(|item| id_of(item) == id).unwrap_or(false))
    {
        *slot = Some(value);
        return Ok(());
    }
    if let Some(slot) = items.iter_mut().find(|slot| slot.is_none()) {
        *slot = Some(value);
        return Ok(());
    }
    Err(StorageError::Full)
}

fn delete_by_id<T: Copy, I: Eq>(
    items: &mut [Option<T>],
    id_of: impl Fn(T) -> I,
    id: I,
) -> Result<(), StorageError> {
    if let Some(slot) = items
        .iter_mut()
        .find(|slot| slot.map(|item| id_of(item) == id).unwrap_or(false))
    {
        *slot = None;
        Ok(())
    } else {
        Err(StorageError::NotFound)
    }
}
