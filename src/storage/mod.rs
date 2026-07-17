use core::cell::RefCell;

use crate::config;
use crate::figures::catalog::skylanders_catalog_entry;
use crate::figures::formats::ImageFormat;
use crate::figures::init::initialize_skylanders_placeholder;
use crate::figures::{FigureKind, GameLine};
#[cfg(target_arch = "xtensa")]
use crate::platform::println;
#[cfg(target_arch = "xtensa")]
use crate::platform::StorageFlash;
use crate::storage::records::{
    BlobId, CharacterIdentity, Entity, EntityDataMode, FixedText, RecordId, StoredBlob,
    MAX_ENTITIES, MAX_IDENTITIES, MAX_RECORD_NAME_BYTES,
};
use crate::storage::wear::{
    DEFAULT_COMMIT_DEBOUNCE_MS, JOURNAL_RECORD_HEADER_BYTES, JOURNAL_RECORD_MAGIC,
};
use crate::usb::skylanders::MAX_FIGURES;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use critical_section::Mutex;
#[cfg(target_arch = "xtensa")]
use embassy_time::{Duration, Timer};

pub mod records;
pub mod wear;

const RECORD_KIND_IDENTITY_UPSERT: u8 = 1;
const RECORD_KIND_IDENTITY_DELETE: u8 = 2;
const RECORD_KIND_ENTITY_UPSERT: u8 = 3;
const RECORD_KIND_ENTITY_DELETE: u8 = 4;
const RECORD_KIND_BLOB_DATA: u8 = 7;
const RECORD_KIND_CONFIG_UPSERT: u8 = 8;
const RECORD_KIND_FORMAT_MARKER: u8 = 254;

const ERASED_WORD: [u8; 4] = [0xff; 4];

static STORE: Mutex<RefCell<Option<Store>>> = Mutex::new(RefCell::new(None));
const CONFIG_SLOTS_MAGIC: &[u8; 4] = b"SLT1";

#[cfg(not(target_arch = "xtensa"))]
struct StorageFlash {
    bytes: Vec<u8>,
}

#[cfg(not(target_arch = "xtensa"))]
impl StorageFlash {
    fn new() -> Self {
        let mut bytes = Vec::new();
        bytes.resize(config::STORAGE_FLASH_BYTES as usize, 0xff);
        Self { bytes }
    }

    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), ()> {
        if offset % 4 != 0 || bytes.len() % 4 != 0 {
            return Err(());
        }
        let offset = offset as usize;
        let end = offset.checked_add(bytes.len()).ok_or(())?;
        bytes.copy_from_slice(self.bytes.get(offset..end).ok_or(())?);
        Ok(())
    }

    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), ()> {
        if offset % 4 != 0 || bytes.len() % 4 != 0 {
            return Err(());
        }
        let offset = offset as usize;
        let end = offset.checked_add(bytes.len()).ok_or(())?;
        let target = self.bytes.get_mut(offset..end).ok_or(())?;
        target.copy_from_slice(bytes);
        Ok(())
    }

    fn erase(&mut self, from: u32, to: u32) -> Result<(), ()> {
        let range = self.bytes.get_mut(from as usize..to as usize).ok_or(())?;
        range.fill(0xff);
        Ok(())
    }
}

pub fn init() {
    let _ = DEFAULT_COMMIT_DEBOUNCE_MS;
    let mut flash = StorageFlash::new();
    let mut catalog = Catalog::new();
    let scan = scan_flash(&mut flash, &mut catalog);
    if scan.is_err() {
        catalog.needs_format = true;
    }
    #[cfg(not(target_arch = "xtensa"))]
    let _ = scan;
    #[cfg(target_arch = "xtensa")]
    println!(
        "Storage scan: identities={}, entities={}, used={} bytes, status={}",
        catalog.identity_count(),
        catalog.entity_count(),
        catalog.write_offset,
        if scan.is_ok() { "ok" } else { "needs-format" }
    );

    critical_section::with(|cs| {
        STORE.borrow_ref_mut(cs).replace(Store { flash, catalog });
    });
}

#[cfg(target_arch = "xtensa")]
#[embassy_executor::task]
pub async fn run() {
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}

pub fn status_json() -> String {
    with_store(|store| store.catalog.status_json())
        .unwrap_or_else(|| String::from("{\"storage\":\"uninitialized\"}"))
}

pub fn library_json() -> String {
    with_store(|store| store.catalog.library_json())
        .unwrap_or_else(|| String::from("{\"error\":\"storage uninitialized\"}"))
}

pub fn identity_json(id: RecordId) -> Result<String, StorageError> {
    with_store(|store| {
        store
            .catalog
            .identity(id)
            .map(|identity| {
                format!(
                    "{{\"id\":{},\"game\":\"{}\",\"name\":\"{}\",\"character_id\":{},\"variant_id\":{},\"kind\":\"{}\",\"format\":\"{}\",\"source\":\"{}\"}}\n",
                    identity.id.0,
                    identity.game_line.wire_name(),
                    json_escape(identity.name.as_str()),
                    identity.character_id,
                    option_u32_json(identity.variant_id),
                    identity.kind.wire_name(),
                    identity.image_format.wire_name(),
                    json_escape(identity.source_notes.as_str())
                )
            })
            .ok_or(StorageError::NotFound)
    })
    .ok_or(StorageError::Uninitialized)?
}

pub fn active_entity_json() -> String {
    with_store(|store| option_record_id_json(store.catalog.active_entity_id()))
        .unwrap_or_else(|| String::from("null"))
}

pub fn active_entity_id() -> Option<RecordId> {
    with_store(|store| store.catalog.active_entity_id()).flatten()
}

pub fn active_slots_json() -> String {
    with_store(|store| store.catalog.active_slots_json()).unwrap_or_else(|| String::from("[]"))
}

pub fn active_slots_marker() -> ([Option<RecordId>; MAX_FIGURES], u32) {
    with_store(|store| {
        (
            store.catalog.active_slots,
            store.catalog.active_config_generation,
        )
    })
    .unwrap_or(([None; MAX_FIGURES], 0))
}

pub fn active_slot_images() -> Result<Vec<(u8, RecordId, Vec<u8>)>, StorageError> {
    with_store_mut(|store| {
        let mut images = Vec::new();
        for slot in 0..MAX_FIGURES {
            let Some(id) = store.catalog.active_slots[slot] else {
                continue;
            };
            let entity = store.catalog.entity(id).ok_or(StorageError::NotFound)?;
            images.push((slot as u8, id, read_entity_image(store, entity)?));
        }
        Ok(images)
    })
}

pub fn create_identity_from_query(query: &str) -> Result<String, StorageError> {
    let name = query_param(query, "name").ok_or(StorageError::BadRequest)?;
    let character_id = query_param(query, "character_id")
        .and_then(|value| parse_u32(value.as_str()))
        .ok_or(StorageError::BadRequest)?;
    let variant_id = query_param(query, "variant_id").and_then(|value| parse_u32(value.as_str()));
    let source = query_param(query, "source").unwrap_or_default();

    with_store_mut(|store| {
        let id = store.catalog.next_record_id();
        let generation = store.catalog.next_generation();
        let identity = CharacterIdentity {
            id,
            game_line: GameLine::Skylanders,
            name: FixedText::from_str(&name).map_err(|_| StorageError::BadRequest)?,
            character_id,
            variant_id,
            kind: FigureKind::Character,
            image_format: ImageFormat::SkylandersMifare1k,
            source_notes: if source.is_empty() {
                FixedText::empty()
            } else {
                FixedText::from_str(&source).map_err(|_| StorageError::BadRequest)?
            },
            generation,
            checksum: crc32(name.as_bytes()),
        };
        let payload = encode_identity(&identity);
        append_record(
            &mut store.flash,
            &mut store.catalog,
            RECORD_KIND_IDENTITY_UPSERT,
            id.0,
            generation,
            &payload,
        )?;
        store.catalog.upsert_identity(identity)?;
        Ok(format!(
            "{{\"created\":\"identity\",\"id\":{},\"name\":\"{}\"}}\n",
            id.0,
            json_escape(identity.name.as_str())
        ))
    })
}

pub fn create_identity_from_params(params: &str) -> Result<String, StorageError> {
    create_identity_from_query(params)
}

pub fn create_entity_from_query(query: &str) -> Result<String, StorageError> {
    let identity_id = query_param(query, "identity_id")
        .and_then(|value| parse_u32(value.as_str()))
        .ok_or(StorageError::BadRequest)?;
    let name = query_param(query, "name").ok_or(StorageError::BadRequest)?;

    with_store_mut(|store| {
        let identity = store
            .catalog
            .identity(RecordId(identity_id))
            .ok_or(StorageError::NotFound)?;
        let image = initialize_skylanders_placeholder(identity.character_id, identity.variant_id);
        let blob_id = append_blob(&mut store.flash, &mut store.catalog, &image)?;

        let image_crc32 = crc32(&image);
        let entity_id = store.catalog.next_record_id();
        let entity_generation = store.catalog.next_generation();
        let entity = Entity {
            id: entity_id,
            name: FixedText::from_str(&name).map_err(|_| StorageError::BadRequest)?,
            parent_identity_id: Some(identity.id),
            catalog_index: None,
            game_line: identity.game_line,
            kind: identity.kind,
            data_mode: EntityDataMode::MutableImage,
            character_id: identity.character_id,
            variant_id: identity.variant_id,
            blob_id: Some(blob_id),
            image_format: identity.image_format,
            image_len: image.len() as u32,
            image_crc32,
            created_generation: entity_generation,
            updated_generation: entity_generation,
        };
        append_entity_record(store, entity)?;
        Ok(format!(
            "{{\"created\":\"entity\",\"id\":{},\"blob_id\":{},\"name\":\"{}\"}}\n",
            entity_id.0,
            blob_id.0,
            json_escape(entity.name.as_str())
        ))
    })
}

pub fn create_entity_from_params(params: &str) -> Result<String, StorageError> {
    create_entity_from_query(params)
}

pub fn create_entity_from_catalog_params(params: &str) -> Result<String, StorageError> {
    let catalog_index = query_param(params, "catalog_index")
        .and_then(|value| parse_u32(value.as_str()))
        .and_then(|value| u16::try_from(value).ok())
        .ok_or(StorageError::BadRequest)?;
    let name = query_param(params, "name").ok_or(StorageError::BadRequest)?;
    let entry = skylanders_catalog_entry(catalog_index).ok_or(StorageError::NotFound)?;

    with_store_mut(|store| {
        let variant_id = if entry.has_variant() {
            Some(entry.variant_id)
        } else {
            None
        };
        let image = initialize_skylanders_placeholder(entry.character_id, variant_id);
        let (data_mode, blob_id, image_len, image_crc32) = if entity_kind_is_mutable(entry.kind) {
            let blob_id = append_blob(&mut store.flash, &mut store.catalog, &image)?;
            (
                EntityDataMode::MutableImage,
                Some(blob_id),
                image.len() as u32,
                crc32(&image),
            )
        } else {
            (
                EntityDataMode::StaticGenerated,
                None,
                image.len() as u32,
                crc32(&image),
            )
        };
        let entity_id = store.catalog.next_record_id();
        let generation = store.catalog.next_generation();
        let entity = Entity {
            id: entity_id,
            name: FixedText::from_str(&name).map_err(|_| StorageError::BadRequest)?,
            parent_identity_id: None,
            catalog_index: Some(entry.index),
            game_line: entry.game_line,
            kind: entry.kind,
            data_mode,
            character_id: entry.character_id,
            variant_id,
            blob_id,
            image_format: ImageFormat::SkylandersMifare1k,
            image_len,
            image_crc32,
            created_generation: generation,
            updated_generation: generation,
        };
        append_entity_record(store, entity)?;
        Ok(format!(
            "{{\"created\":\"entity\",\"id\":{},\"catalog_index\":{},\"data_mode\":\"{}\",\"blob_id\":{},\"name\":\"{}\",\"figure\":\"{}\"}}\n",
            entity_id.0,
            entry.index,
            entity.data_mode.wire_name(),
            option_blob_id_json(blob_id),
            json_escape(entity.name.as_str()),
            json_escape(entry.name)
        ))
    })
}

pub fn upload_entity_from_params(params: &str, image: &[u8]) -> Result<String, StorageError> {
    let name = query_param(params, "name").ok_or(StorageError::BadRequest)?;
    let identity_id =
        query_param(params, "identity_id").and_then(|value| parse_u32(value.as_str()));
    if image.is_empty() {
        return Err(StorageError::BadRequest);
    }

    with_store_mut(|store| {
        let identity = identity_id.and_then(|id| store.catalog.identity(RecordId(id)));
        let game_line = identity
            .map(|item| item.game_line)
            .unwrap_or(GameLine::Skylanders);
        let image_format = identity
            .map(|item| item.image_format)
            .unwrap_or(ImageFormat::SkylandersMifare1k);
        let blob_id = append_blob(&mut store.flash, &mut store.catalog, image)?;
        let image_crc32 = crc32(image);
        let entity_id = store.catalog.next_record_id();
        let generation = store.catalog.next_generation();
        let entity = Entity {
            id: entity_id,
            name: FixedText::from_str(&name).map_err(|_| StorageError::BadRequest)?,
            parent_identity_id: identity.map(|item| item.id),
            catalog_index: None,
            game_line,
            kind: identity
                .map(|item| item.kind)
                .unwrap_or(FigureKind::Unknown),
            data_mode: EntityDataMode::MutableImage,
            character_id: identity.map(|item| item.character_id).unwrap_or(0),
            variant_id: identity.and_then(|item| item.variant_id),
            blob_id: Some(blob_id),
            image_format,
            image_len: image.len() as u32,
            image_crc32,
            created_generation: generation,
            updated_generation: generation,
        };
        append_entity_record(store, entity)?;
        Ok(format!(
            "{{\"uploaded\":\"entity\",\"id\":{},\"blob_id\":{},\"name\":\"{}\"}}\n",
            entity_id.0,
            blob_id.0,
            json_escape(entity.name.as_str())
        ))
    })
}

pub fn clone_entity_from_params(params: &str) -> Result<String, StorageError> {
    let source_id = query_param(params, "source_id")
        .or_else(|| query_param(params, "id"))
        .and_then(|value| parse_u32(value.as_str()))
        .ok_or(StorageError::BadRequest)?;
    let name = query_param(params, "name").ok_or(StorageError::BadRequest)?;

    with_store_mut(|store| {
        let source = store
            .catalog
            .entity(RecordId(source_id))
            .ok_or(StorageError::NotFound)?;
        let image = read_entity_image(store, source)?;
        let blob_id = append_blob(&mut store.flash, &mut store.catalog, &image)?;
        let id = store.catalog.next_record_id();
        let generation = store.catalog.next_generation();
        let clone = Entity {
            id,
            name: FixedText::from_str(&name).map_err(|_| StorageError::BadRequest)?,
            parent_identity_id: source.parent_identity_id,
            catalog_index: source.catalog_index,
            game_line: source.game_line,
            kind: source.kind,
            data_mode: EntityDataMode::MutableImage,
            character_id: source.character_id,
            variant_id: source.variant_id,
            blob_id: Some(blob_id),
            image_format: source.image_format,
            image_len: source.image_len,
            image_crc32: source.image_crc32,
            created_generation: generation,
            updated_generation: generation,
        };
        append_entity_record(store, clone)?;
        Ok(format!(
            "{{\"cloned\":\"entity\",\"source_id\":{},\"id\":{},\"blob_id\":{},\"name\":\"{}\"}}\n",
            source_id,
            id.0,
            blob_id.0,
            json_escape(clone.name.as_str())
        ))
    })
}

pub fn delete_identity_from_query(query: &str) -> Result<String, StorageError> {
    delete_record_from_query(
        query,
        "identity",
        RECORD_KIND_IDENTITY_DELETE,
        |catalog, id| catalog.delete_identity(id),
    )
}

pub fn rename_identity_from_query(query: &str) -> Result<String, StorageError> {
    let id = query_param(query, "id")
        .and_then(|value| parse_u32(value.as_str()))
        .ok_or(StorageError::BadRequest)?;
    let name = query_param(query, "name").ok_or(StorageError::BadRequest)?;

    with_store_mut(|store| {
        let mut identity = store
            .catalog
            .identity(RecordId(id))
            .ok_or(StorageError::NotFound)?;
        identity.name = FixedText::from_str(&name).map_err(|_| StorageError::BadRequest)?;
        identity.generation = store.catalog.next_generation();
        append_record(
            &mut store.flash,
            &mut store.catalog,
            RECORD_KIND_IDENTITY_UPSERT,
            id,
            identity.generation,
            &encode_identity(&identity),
        )?;
        store.catalog.upsert_identity(identity)?;
        Ok(format!("{{\"renamed\":\"identity\",\"id\":{}}}\n", id))
    })
}

pub fn delete_entity_from_query(query: &str) -> Result<String, StorageError> {
    delete_record_from_query(query, "entity", RECORD_KIND_ENTITY_DELETE, |catalog, id| {
        catalog.delete_entity(id)
    })
}

pub fn rename_entity_from_query(query: &str) -> Result<String, StorageError> {
    let id = query_param(query, "id")
        .and_then(|value| parse_u32(value.as_str()))
        .ok_or(StorageError::BadRequest)?;
    let name = query_param(query, "name").ok_or(StorageError::BadRequest)?;

    with_store_mut(|store| {
        let mut entity = store
            .catalog
            .entity(RecordId(id))
            .ok_or(StorageError::NotFound)?;
        entity.name = FixedText::from_str(&name).map_err(|_| StorageError::BadRequest)?;
        entity.updated_generation = store.catalog.next_generation();
        let payload = encode_entity(&entity);
        append_record(
            &mut store.flash,
            &mut store.catalog,
            RECORD_KIND_ENTITY_UPSERT,
            id,
            entity.updated_generation,
            &payload,
        )?;
        store.catalog.upsert_entity(entity)?;
        Ok(format!("{{\"renamed\":\"entity\",\"id\":{}}}\n", id))
    })
}

pub fn read_entity_blob(entity_id: RecordId) -> Result<Vec<u8>, StorageError> {
    with_store_mut(|store| {
        let entity = store
            .catalog
            .entity(entity_id)
            .ok_or(StorageError::NotFound)?;
        read_entity_image(store, entity)
    })
}

pub fn replace_entity_blob(entity_id: RecordId, image: &[u8]) -> Result<(), StorageError> {
    with_store_mut(|store| {
        let mut entity = store
            .catalog
            .entity(entity_id)
            .ok_or(StorageError::NotFound)?;
        let blob_id = append_blob(&mut store.flash, &mut store.catalog, image)?;
        entity.blob_id = Some(blob_id);
        entity.data_mode = EntityDataMode::MutableImage;
        entity.image_len = image.len() as u32;
        entity.image_crc32 = crc32(image);
        entity.updated_generation = store.catalog.next_generation();
        append_entity_record(store, entity)?;
        Ok(())
    })
}

pub fn select_entity_from_params(params: &str) -> Result<String, StorageError> {
    let id = query_param(params, "id")
        .and_then(|value| parse_u32(value.as_str()))
        .ok_or(StorageError::BadRequest)?;
    let slot = query_param(params, "slot")
        .and_then(|value| parse_u32(value.as_str()))
        .unwrap_or(0);
    if slot as usize >= MAX_FIGURES {
        return Err(StorageError::BadRequest);
    }

    with_store_mut(|store| {
        let id = RecordId(id);
        if store.catalog.entity(id).is_none() {
            return Err(StorageError::NotFound);
        }
        let generation = store.catalog.next_generation();
        store.catalog.active_slots[slot as usize] = Some(id);
        let active_slots = store.catalog.active_slots;
        append_config_record(
            &mut store.flash,
            &mut store.catalog,
            active_slots,
            generation,
        )?;
        store.catalog.active_config_generation = generation;
        Ok(format!(
            "{{\"active_entity_id\":{},\"slot\":{},\"active_slots\":{}}}\n",
            id.0,
            slot,
            store.catalog.active_slots_json()
        ))
    })
}

pub fn clear_active_entity() -> Result<String, StorageError> {
    clear_active_entity_from_params("")
}

pub fn clear_active_entity_from_params(params: &str) -> Result<String, StorageError> {
    let slot = query_param(params, "slot").and_then(|value| parse_u32(value.as_str()));
    if slot.is_some_and(|slot| slot as usize >= MAX_FIGURES) {
        return Err(StorageError::BadRequest);
    }

    with_store_mut(|store| {
        let generation = store.catalog.next_generation();
        if let Some(slot) = slot {
            store.catalog.active_slots[slot as usize] = None;
        } else {
            store.catalog.active_slots = [None; MAX_FIGURES];
        }
        let active_slots = store.catalog.active_slots;
        append_config_record(
            &mut store.flash,
            &mut store.catalog,
            active_slots,
            generation,
        )?;
        store.catalog.active_config_generation = generation;
        Ok(format!(
            "{{\"active_entity_id\":{},\"active_slots\":{}}}\n",
            option_record_id_json(store.catalog.active_entity_id()),
            store.catalog.active_slots_json()
        ))
    })
}

pub fn compact_storage() -> Result<String, StorageError> {
    with_store_mut(|store| {
        let mut blob_ids = Vec::new();
        for blob in store.catalog.blobs.iter().flatten() {
            let is_live_entity = store
                .catalog
                .entities
                .iter()
                .flatten()
                .any(|entity| entity.blob_id == Some(blob.id));
            if is_live_entity {
                blob_ids.push(blob.id);
            }
        }
        let mut blobs = Vec::new();
        for blob_id in blob_ids {
            blobs.push((blob_id, read_blob(store, blob_id)?));
        }

        let identities = store.catalog.identities;
        let entities = store.catalog.entities;
        let active_slots = store.catalog.active_slots;
        let active_config_generation = store.catalog.active_config_generation;
        let next_record_id = store.catalog.next_record_id;
        let next_blob_id = store.catalog.next_blob_id;
        let mut generation = store.catalog.next_generation;

        store
            .flash
            .erase(
                config::STORAGE_FLASH_OFFSET,
                config::STORAGE_FLASH_OFFSET + config::STORAGE_FLASH_BYTES,
            )
            .map_err(|_| StorageError::Flash)?;
        store.catalog = Catalog::new();
        store.catalog.next_record_id = next_record_id;
        store.catalog.next_blob_id = next_blob_id;
        store.catalog.next_generation = generation;

        append_record(
            &mut store.flash,
            &mut store.catalog,
            RECORD_KIND_FORMAT_MARKER,
            0,
            generation,
            b"omniportal-storage-v1",
        )?;
        generation += 1;

        for (blob_id, image) in blobs {
            append_record(
                &mut store.flash,
                &mut store.catalog,
                RECORD_KIND_BLOB_DATA,
                blob_id.0,
                generation,
                &image,
            )?;
            store.catalog.upsert_blob(StoredBlob {
                id: blob_id,
                offset: store.catalog.write_offset
                    - align4(JOURNAL_RECORD_HEADER_BYTES as u32 + image.len() as u32)
                    + JOURNAL_RECORD_HEADER_BYTES as u32,
                len: image.len() as u32,
                crc32: crc32(&image),
                generation,
            })?;
            generation += 1;
        }

        for identity in identities.iter().flatten().copied() {
            let mut identity = identity;
            identity.generation = generation;
            append_record(
                &mut store.flash,
                &mut store.catalog,
                RECORD_KIND_IDENTITY_UPSERT,
                identity.id.0,
                generation,
                &encode_identity(&identity),
            )?;
            store.catalog.upsert_identity(identity)?;
            generation += 1;
        }

        for entity in entities.iter().flatten().copied() {
            let mut entity = entity;
            entity.updated_generation = generation;
            append_record(
                &mut store.flash,
                &mut store.catalog,
                RECORD_KIND_ENTITY_UPSERT,
                entity.id.0,
                generation,
                &encode_entity(&entity),
            )?;
            store.catalog.upsert_entity(entity)?;
            generation += 1;
        }

        if active_slots.iter().any(Option::is_some) {
            append_config_record(
                &mut store.flash,
                &mut store.catalog,
                active_slots,
                generation,
            )?;
            store.catalog.active_slots = active_slots;
            store.catalog.active_config_generation = generation;
            generation += 1;
        } else {
            store.catalog.active_config_generation = active_config_generation;
        }

        store.catalog.next_generation = generation;
        Ok(format!(
            "{{\"compacted\":true,\"used_bytes\":{}}}\n",
            store.catalog.write_offset
        ))
    })
}

pub fn format_storage() -> Result<String, StorageError> {
    with_store_mut(|store| {
        store
            .flash
            .erase(
                config::STORAGE_FLASH_OFFSET,
                config::STORAGE_FLASH_OFFSET + config::STORAGE_FLASH_BYTES,
            )
            .map_err(|_| StorageError::Flash)?;
        store.catalog = Catalog::new();
        append_record(
            &mut store.flash,
            &mut store.catalog,
            RECORD_KIND_FORMAT_MARKER,
            0,
            1,
            b"omniportal-storage-v1",
        )?;
        Ok(String::from("{\"formatted\":true}\n"))
    })
}

fn delete_record_from_query(
    query: &str,
    label: &str,
    record_kind: u8,
    apply: impl FnOnce(&mut Catalog, RecordId) -> Result<(), StorageError>,
) -> Result<String, StorageError> {
    let id = query_param(query, "id")
        .and_then(|value| parse_u32(value.as_str()))
        .ok_or(StorageError::BadRequest)?;

    with_store_mut(|store| {
        let generation = store.catalog.next_generation();
        append_record(
            &mut store.flash,
            &mut store.catalog,
            record_kind,
            id,
            generation,
            &[],
        )?;
        apply(&mut store.catalog, RecordId(id))?;
        Ok(format!("{{\"deleted\":\"{}\",\"id\":{}}}\n", label, id))
    })
}

fn read_blob(store: &mut Store, blob_id: BlobId) -> Result<Vec<u8>, StorageError> {
    let blob = store.catalog.blob(blob_id).ok_or(StorageError::NotFound)?;
    let mut data = Vec::new();
    data.resize(blob.len as usize, 0);
    store
        .flash
        .read(config::STORAGE_FLASH_OFFSET + blob.offset, &mut data)
        .map_err(|_| StorageError::Flash)?;
    if crc32(&data) != blob.crc32 {
        return Err(StorageError::Corrupt);
    }
    Ok(data)
}

fn read_entity_image(store: &mut Store, entity: Entity) -> Result<Vec<u8>, StorageError> {
    if let Some(blob_id) = entity.blob_id {
        return read_blob(store, blob_id);
    }
    Ok(generated_entity_image(entity))
}

fn generated_entity_image(entity: Entity) -> Vec<u8> {
    let image = initialize_skylanders_placeholder(entity.character_id, entity.variant_id);
    let mut out = Vec::new();
    out.extend_from_slice(&image);
    out
}

fn entity_kind_is_mutable(kind: FigureKind) -> bool {
    matches!(
        kind,
        FigureKind::Character
            | FigureKind::Trap
            | FigureKind::CreationCrystal
            | FigureKind::Vehicle
            | FigureKind::Trophy
    )
}

fn with_store<R>(f: impl FnOnce(&Store) -> R) -> Option<R> {
    critical_section::with(|cs| STORE.borrow_ref(cs).as_ref().map(f))
}

fn with_store_mut<R>(
    f: impl FnOnce(&mut Store) -> Result<R, StorageError>,
) -> Result<R, StorageError> {
    critical_section::with(|cs| {
        let mut slot = STORE.borrow_ref_mut(cs);
        let store = slot.as_mut().ok_or(StorageError::Uninitialized)?;
        f(store)
    })
}

struct Store {
    flash: StorageFlash,
    catalog: Catalog,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageError {
    Uninitialized,
    NeedsFormat,
    BadRequest,
    NotFound,
    Full,
    Flash,
    Corrupt,
}

impl StorageError {
    pub const fn status_code(self) -> &'static str {
        match self {
            Self::BadRequest => "400 Bad Request",
            Self::NotFound => "404 Not Found",
            Self::NeedsFormat => "409 Conflict",
            Self::Full => "507 Insufficient Storage",
            Self::Uninitialized | Self::Flash | Self::Corrupt => "500 Internal Server Error",
        }
    }

    pub const fn message(self) -> &'static str {
        match self {
            Self::Uninitialized => "storage uninitialized",
            Self::NeedsFormat => "storage needs format",
            Self::BadRequest => "bad request",
            Self::NotFound => "not found",
            Self::Full => "storage full",
            Self::Flash => "flash error",
            Self::Corrupt => "stored entity data is corrupt",
        }
    }
}

#[derive(Clone, Copy)]
struct Catalog {
    identities: [Option<CharacterIdentity>; MAX_IDENTITIES],
    entities: [Option<Entity>; MAX_ENTITIES],
    blobs: [Option<StoredBlob>; MAX_ENTITIES],
    active_slots: [Option<RecordId>; MAX_FIGURES],
    active_config_generation: u32,
    needs_format: bool,
    write_offset: u32,
    next_record_id: u32,
    next_blob_id: u32,
    next_generation: u32,
    corrupt_records: u32,
}

impl Catalog {
    const fn new() -> Self {
        Self {
            identities: [None; MAX_IDENTITIES],
            entities: [None; MAX_ENTITIES],
            blobs: [None; MAX_ENTITIES],
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

    fn next_record_id(&mut self) -> RecordId {
        let id = self.next_record_id;
        self.next_record_id += 1;
        RecordId(id)
    }

    fn next_blob_id(&mut self) -> BlobId {
        let id = self.next_blob_id;
        self.next_blob_id += 1;
        BlobId(id)
    }

    fn next_generation(&mut self) -> u32 {
        let generation = self.next_generation;
        self.next_generation += 1;
        generation
    }

    fn observe_record_id(&mut self, id: u32, generation: u32) {
        self.next_record_id = self.next_record_id.max(id.saturating_add(1));
        self.next_generation = self.next_generation.max(generation.saturating_add(1));
    }

    fn observe_blob_id(&mut self, id: u32, generation: u32) {
        self.next_blob_id = self.next_blob_id.max(id.saturating_add(1));
        self.next_generation = self.next_generation.max(generation.saturating_add(1));
    }

    fn identity(&self, id: RecordId) -> Option<CharacterIdentity> {
        self.identities
            .iter()
            .flatten()
            .find(|item| item.id == id)
            .copied()
    }

    fn entity(&self, id: RecordId) -> Option<Entity> {
        self.entities
            .iter()
            .flatten()
            .find(|item| item.id == id)
            .copied()
    }

    fn blob(&self, id: BlobId) -> Option<StoredBlob> {
        self.blobs
            .iter()
            .flatten()
            .find(|item| item.id == id)
            .copied()
    }

    fn upsert_identity(&mut self, identity: CharacterIdentity) -> Result<(), StorageError> {
        upsert_by_id(&mut self.identities, identity, |item| item.id, identity.id)
    }

    fn upsert_entity(&mut self, entity: Entity) -> Result<(), StorageError> {
        upsert_by_id(&mut self.entities, entity, |item| item.id, entity.id)
    }

    fn upsert_blob(&mut self, blob: StoredBlob) -> Result<(), StorageError> {
        upsert_by_id(&mut self.blobs, blob, |item| item.id, blob.id)
    }

    fn delete_identity(&mut self, id: RecordId) -> Result<(), StorageError> {
        delete_by_id(&mut self.identities, |item| item.id, id)
    }

    fn delete_entity(&mut self, id: RecordId) -> Result<(), StorageError> {
        for slot in &mut self.active_slots {
            if *slot == Some(id) {
                *slot = None;
            }
        }
        delete_by_id(&mut self.entities, |item| item.id, id)
    }

    fn active_entity_id(&self) -> Option<RecordId> {
        self.active_slots.iter().find_map(|id| *id)
    }

    fn active_slots_json(&self) -> String {
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

    fn identity_count(&self) -> usize {
        self.identities.iter().filter(|item| item.is_some()).count()
    }

    fn entity_count(&self) -> usize {
        self.entities.iter().filter(|item| item.is_some()).count()
    }

    fn status_json(&self) -> String {
        format!(
            "{{\"storage\":\"{}\",\"identities\":{},\"entities\":{},\"active_entity_id\":{},\"active_slots\":{},\"used_bytes\":{},\"capacity_bytes\":{},\"corrupt_records\":{}}}",
            if self.needs_format {
                "needs-format"
            } else {
                "ok"
            },
            self.identity_count(),
            self.entity_count(),
            option_record_id_json(self.active_entity_id()),
            self.active_slots_json(),
            self.write_offset,
            config::STORAGE_FLASH_BYTES,
            self.corrupt_records
        )
    }

    fn library_json(&self) -> String {
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
            out.push_str(&format!(
                "{{\"id\":{},\"name\":\"{}\",\"identity_id\":{},\"catalog_index\":{},\"game\":\"{}\",\"kind\":\"{}\",\"data_mode\":\"{}\",\"character_id\":{},\"variant_id\":{},\"blob_id\":{},\"image_len\":{},\"crc32\":{}}}",
                entity.id.0,
                json_escape(entity.name.as_str()),
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

fn scan_flash(flash: &mut StorageFlash, catalog: &mut Catalog) -> Result<(), StorageError> {
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
            catalog.active_slots = decode_config(payload);
            catalog.active_config_generation = record.generation;
        }
        _ => {}
    }
    Ok(())
}

fn append_blob(
    flash: &mut StorageFlash,
    catalog: &mut Catalog,
    image: &[u8],
) -> Result<BlobId, StorageError> {
    let blob_id = catalog.next_blob_id();
    let generation = catalog.next_generation();
    let payload_offset = catalog.write_offset + JOURNAL_RECORD_HEADER_BYTES as u32;
    append_record(
        flash,
        catalog,
        RECORD_KIND_BLOB_DATA,
        blob_id.0,
        generation,
        image,
    )?;
    catalog.upsert_blob(StoredBlob {
        id: blob_id,
        offset: payload_offset,
        len: image.len() as u32,
        crc32: crc32(image),
        generation,
    })?;
    Ok(blob_id)
}

fn append_entity_record(store: &mut Store, entity: Entity) -> Result<(), StorageError> {
    append_record(
        &mut store.flash,
        &mut store.catalog,
        RECORD_KIND_ENTITY_UPSERT,
        entity.id.0,
        entity.updated_generation,
        &encode_entity(&entity),
    )?;
    store.catalog.upsert_entity(entity)
}

fn append_config_record(
    flash: &mut StorageFlash,
    catalog: &mut Catalog,
    active_slots: [Option<RecordId>; MAX_FIGURES],
    generation: u32,
) -> Result<(), StorageError> {
    let payload = encode_config(active_slots);
    append_record(
        flash,
        catalog,
        RECORD_KIND_CONFIG_UPSERT,
        0,
        generation,
        &payload,
    )
}

fn append_record(
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

struct JournalHeader {
    kind: u8,
    id: u32,
    generation: u32,
    payload_len: u32,
    payload_crc: u32,
}

impl JournalHeader {
    fn encode(&self) -> [u8; JOURNAL_RECORD_HEADER_BYTES] {
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

    fn decode(bytes: &[u8; JOURNAL_RECORD_HEADER_BYTES]) -> Option<Self> {
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

fn encode_identity(identity: &CharacterIdentity) -> [u8; 176] {
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

fn decode_identity(id: u32, generation: u32, payload: &[u8]) -> Option<CharacterIdentity> {
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

fn encode_entity(entity: &Entity) -> [u8; 128] {
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

fn decode_entity(id: u32, _generation: u32, payload: &[u8]) -> Option<Entity> {
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

fn encode_config(active_slots: [Option<RecordId>; MAX_FIGURES]) -> [u8; 4 + MAX_FIGURES * 4] {
    let mut out = [0; 4 + MAX_FIGURES * 4];
    out[0..4].copy_from_slice(CONFIG_SLOTS_MAGIC);
    for (slot, entity_id) in active_slots.iter().enumerate() {
        let start = 4 + slot * 4;
        out[start..start + 4].copy_from_slice(&entity_id.map(|id| id.0).unwrap_or(0).to_le_bytes());
    }
    out
}

fn decode_config(payload: &[u8]) -> [Option<RecordId>; MAX_FIGURES] {
    let mut active_slots = [None; MAX_FIGURES];
    if payload.len() >= 4 + MAX_FIGURES * 4 && &payload[0..4] == CONFIG_SLOTS_MAGIC {
        for (slot, active_slot) in active_slots.iter_mut().enumerate() {
            let start = 4 + slot * 4;
            let id = u32::from_le_bytes(payload[start..start + 4].try_into().unwrap_or([0; 4]));
            if id != 0 {
                *active_slot = Some(RecordId(id));
            }
        }
        return active_slots;
    }

    if payload.len() >= 8 && payload[0] != 0 {
        if let Ok(bytes) = payload[4..8].try_into() {
            let id = u32::from_le_bytes(bytes);
            if id != 0 {
                active_slots[0] = Some(RecordId(id));
            }
        }
    }
    active_slots
}

pub fn crc32(bytes: &[u8]) -> u32 {
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

fn align4(value: u32) -> u32 {
    (value + 3) & !3
}

fn query_param(query: &str, name: &str) -> Option<String> {
    for pair in query.split('&') {
        if let Some((key, value)) = pair.split_once('=') {
            if key == name {
                return Some(percent_decode(value));
            }
        }
    }
    None
}

fn parse_u32(value: &str) -> Option<u32> {
    if let Some(hex) = value.strip_prefix("0x") {
        u32::from_str_radix(hex, 16).ok()
    } else {
        value.parse().ok()
    }
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

fn json_escape(value: &str) -> String {
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

fn option_u32_json(value: Option<u32>) -> String {
    value
        .map(|value| format!("{}", value))
        .unwrap_or_else(|| String::from("null"))
}

fn option_u16_json(value: Option<u16>) -> String {
    value
        .map(|value| format!("{}", value))
        .unwrap_or_else(|| String::from("null"))
}

fn option_record_id_json(value: Option<RecordId>) -> String {
    value
        .map(|value| format!("{}", value.0))
        .unwrap_or_else(|| String::from("null"))
}

fn option_blob_id_json(value: Option<BlobId>) -> String {
    value
        .map(|value| format!("{}", value.0))
        .unwrap_or_else(|| String::from("null"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_identity() -> CharacterIdentity {
        CharacterIdentity {
            id: RecordId(7),
            game_line: GameLine::Skylanders,
            name: FixedText::from_str("Trigger Happy").unwrap(),
            character_id: 21,
            variant_id: Some(3),
            kind: FigureKind::Character,
            image_format: ImageFormat::SkylandersMifare1k,
            source_notes: FixedText::from_str("seeded").unwrap(),
            generation: 11,
            checksum: 0x1234_5678,
        }
    }

    fn test_entity() -> Entity {
        Entity {
            id: RecordId(8),
            name: FixedText::from_str("Preston's Trigger Happy").unwrap(),
            parent_identity_id: Some(RecordId(7)),
            catalog_index: Some(19),
            game_line: GameLine::Skylanders,
            kind: FigureKind::Character,
            data_mode: EntityDataMode::MutableImage,
            character_id: 21,
            variant_id: Some(3),
            blob_id: Some(BlobId(2)),
            image_format: ImageFormat::SkylandersMifare1k,
            image_len: 1024,
            image_crc32: 0xabcd_1234,
            created_generation: 12,
            updated_generation: 13,
        }
    }

    #[test]
    fn decodes_form_params_and_numbers() {
        assert_eq!(
            query_param("name=Preston%27s+Trigger+Happy&character_id=0x15", "name")
                .unwrap()
                .as_str(),
            "Preston's Trigger Happy"
        );
        assert_eq!(
            query_param(
                "name=Preston%27s+Trigger+Happy&character_id=0x15",
                "character_id"
            )
            .and_then(|value| parse_u32(value.as_str())),
            Some(21)
        );
        assert_eq!(
            query_param("broken&name=ok", "name"),
            Some(String::from("ok"))
        );
    }

    #[test]
    fn rejects_empty_and_oversized_record_text() {
        assert!(FixedText::<8>::from_str("").is_err());
        assert!(FixedText::<8>::from_str("123456789").is_err());
        assert_eq!(
            FixedText::<8>::from_str("Trigger").unwrap().as_str(),
            "Trigger"
        );
    }

    #[test]
    fn journal_header_round_trips() {
        let header = JournalHeader {
            kind: RECORD_KIND_IDENTITY_UPSERT,
            id: 7,
            generation: 11,
            payload_len: 176,
            payload_crc: 0xdead_beef,
        };

        let decoded = JournalHeader::decode(&header.encode()).unwrap();

        assert_eq!(decoded.kind, header.kind);
        assert_eq!(decoded.id, header.id);
        assert_eq!(decoded.generation, header.generation);
        assert_eq!(decoded.payload_len, header.payload_len);
        assert_eq!(decoded.payload_crc, header.payload_crc);
    }

    #[test]
    fn identity_and_entity_payloads_round_trip() {
        let identity = test_identity();
        let entity = test_entity();

        assert_eq!(
            decode_identity(
                identity.id.0,
                identity.generation,
                &encode_identity(&identity)
            ),
            Some(identity)
        );
        assert_eq!(
            decode_entity(
                entity.id.0,
                entity.updated_generation,
                &encode_entity(&entity)
            ),
            Some(entity)
        );
    }

    #[test]
    fn config_payload_supports_legacy_and_multi_slot_records() {
        let legacy = [1, 0, 0, 0, 42, 0, 0, 0];
        let decoded = decode_config(&legacy);
        assert_eq!(decoded[0], Some(RecordId(42)));
        assert_eq!(decoded[1], None);

        let mut active_slots = [None; MAX_FIGURES];
        active_slots[0] = Some(RecordId(8));
        active_slots[3] = Some(RecordId(11));

        let decoded = decode_config(&encode_config(active_slots));
        assert_eq!(decoded[0], Some(RecordId(8)));
        assert_eq!(decoded[3], Some(RecordId(11)));
        assert_eq!(decoded[4], None);
    }

    #[test]
    fn catalog_upsert_replace_delete_and_active_selection_behave() {
        let mut catalog = Catalog::new();
        let mut identity = test_identity();
        catalog.upsert_identity(identity).unwrap();
        assert_eq!(catalog.identity_count(), 1);

        identity.name = FixedText::from_str("Renamed").unwrap();
        catalog.upsert_identity(identity).unwrap();
        assert_eq!(catalog.identity_count(), 1);
        assert_eq!(
            catalog.identity(identity.id).unwrap().name.as_str(),
            "Renamed"
        );

        let entity = test_entity();
        catalog.upsert_entity(entity).unwrap();
        catalog.active_slots[0] = Some(entity.id);
        catalog.delete_entity(entity.id).unwrap();
        assert_eq!(catalog.entity_count(), 0);
        assert_eq!(catalog.active_entity_id(), None);
    }

    #[test]
    fn append_and_scan_flash_rebuilds_catalog_after_reboot() {
        let mut flash = StorageFlash::new();
        let mut catalog = Catalog::new();
        let image = [0x42; 16];

        append_record(
            &mut flash,
            &mut catalog,
            RECORD_KIND_FORMAT_MARKER,
            0,
            1,
            b"omniportal-storage-v1",
        )
        .unwrap();
        append_record(
            &mut flash,
            &mut catalog,
            RECORD_KIND_IDENTITY_UPSERT,
            7,
            2,
            &encode_identity(&test_identity()),
        )
        .unwrap();
        append_record(
            &mut flash,
            &mut catalog,
            RECORD_KIND_BLOB_DATA,
            2,
            3,
            &image,
        )
        .unwrap();
        append_record(
            &mut flash,
            &mut catalog,
            RECORD_KIND_ENTITY_UPSERT,
            8,
            4,
            &encode_entity(&test_entity()),
        )
        .unwrap();
        let mut active_slots = [None; MAX_FIGURES];
        active_slots[0] = Some(RecordId(8));
        append_config_record(&mut flash, &mut catalog, active_slots, 5).unwrap();

        let used_bytes = catalog.write_offset;
        let mut rebuilt = Catalog::new();
        scan_flash(&mut flash, &mut rebuilt).unwrap();

        assert_eq!(rebuilt.write_offset, used_bytes);
        assert_eq!(
            rebuilt.identity(RecordId(7)).unwrap().name.as_str(),
            "Trigger Happy"
        );
        assert_eq!(
            rebuilt.entity(RecordId(8)).unwrap().name.as_str(),
            "Preston's Trigger Happy"
        );
        assert_eq!(rebuilt.blob(BlobId(2)).unwrap().len, image.len() as u32);
        assert_eq!(rebuilt.active_entity_id(), Some(RecordId(8)));
        assert_eq!(rebuilt.active_slots[0], Some(RecordId(8)));
        assert_eq!(rebuilt.next_record_id, 9);
        assert_eq!(rebuilt.next_blob_id, 3);
        assert_eq!(rebuilt.next_generation, 6);
    }

    #[test]
    fn scan_flash_reports_corrupt_payload_crc() {
        let mut flash = StorageFlash::new();
        let mut catalog = Catalog::new();
        append_record(
            &mut flash,
            &mut catalog,
            RECORD_KIND_FORMAT_MARKER,
            0,
            1,
            b"omniportal-storage-v1",
        )
        .unwrap();
        flash.bytes[JOURNAL_RECORD_HEADER_BYTES] ^= 0x01;

        let mut rebuilt = Catalog::new();
        assert_eq!(
            scan_flash(&mut flash, &mut rebuilt),
            Err(StorageError::Corrupt)
        );
        assert_eq!(rebuilt.corrupt_records, 1);
    }

    #[test]
    fn scan_flash_keeps_valid_records_before_torn_tail() {
        let mut flash = StorageFlash::new();
        let mut catalog = Catalog::new();
        append_record(
            &mut flash,
            &mut catalog,
            RECORD_KIND_FORMAT_MARKER,
            0,
            1,
            b"omniportal-storage-v1",
        )
        .unwrap();
        let torn_offset = catalog.write_offset as usize;
        flash.bytes[torn_offset..torn_offset + 4]
            .copy_from_slice(&JOURNAL_RECORD_MAGIC.to_le_bytes());

        let mut rebuilt = Catalog::new();
        scan_flash(&mut flash, &mut rebuilt).unwrap();

        assert_eq!(rebuilt.corrupt_records, 1);
        assert_eq!(rebuilt.write_offset, 4096);
        assert!(!rebuilt.needs_format);
        assert_eq!(rebuilt.next_generation, 2);
    }

    #[test]
    fn append_refuses_writes_when_storage_needs_format() {
        let mut flash = StorageFlash::new();
        let mut catalog = Catalog::new();
        catalog.needs_format = true;

        assert_eq!(
            append_record(
                &mut flash,
                &mut catalog,
                RECORD_KIND_ENTITY_DELETE,
                8,
                1,
                &[]
            ),
            Err(StorageError::NeedsFormat)
        );
        assert_eq!(catalog.write_offset, 0);
    }
}
