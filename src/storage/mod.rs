use core::cell::RefCell;

use crate::config;
use crate::domain::{FigureKind, GameLine, ImageFormat};
use crate::figures::formats::{INFINITY_IMAGE_BYTES, SKYLANDERS_IMAGE_BYTES};
use crate::figures::infinity::{
    find_infinity_catalog_entry, infinity_catalog_entry, infinity_figure_number,
    initialize_infinity_entity_image,
};
use crate::figures::skylanders::catalog::{
    skylanders_catalog_entry, FigureCatalogEntry as SkylandersCatalogEntry, SKYLANDERS_CATALOG,
};
use crate::figures::skylanders::crypto::validate_skylanders_mifare_image;
use crate::figures::skylanders::image::{
    initialize_mutable_skylanders_entity_image, initialize_skylanders_entity_image,
    rekey_skylanders_entity_image,
};
#[cfg(target_arch = "xtensa")]
use crate::platform::println;
#[cfg(target_arch = "xtensa")]
use crate::platform::StorageFlash;
use crate::storage::catalog::Catalog;
use crate::storage::forms::{decode_hex_bytes, parse_game_param, parse_u32, query_param};
use crate::storage::journal::{
    align4, append_record, crc32, encode_config, encode_entity, encode_identity, scan_flash,
    RECORD_KIND_BLOB_DATA, RECORD_KIND_CONFIG_UPSERT, RECORD_KIND_ENTITY_DELETE,
    RECORD_KIND_ENTITY_UPSERT, RECORD_KIND_FORMAT_MARKER, RECORD_KIND_IDENTITY_DELETE,
    RECORD_KIND_IDENTITY_UPSERT,
};
use crate::storage::json::{
    hex_bytes, json_escape, option_blob_id_json, option_record_id_json, option_str_json,
    option_u16_json, option_u32_json,
};
use crate::storage::records::{
    BlobId, CharacterIdentity, Entity, EntityDataMode, FixedText, RecordId, StoredBlob,
};
use crate::storage::wear::{
    DEFAULT_COMMIT_DEBOUNCE_MS, JOURNAL_RECORD_HEADER_BYTES, PROACTIVE_COMPACT_USED_PERCENT,
};
use crate::usb::skylanders::MAX_FIGURES;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use critical_section::Mutex;
#[cfg(target_arch = "xtensa")]
use embassy_time::{Duration, Timer};

mod catalog;
mod forms;
mod journal;
mod json;
pub mod records;
pub mod wear;

static STORE: Mutex<RefCell<Option<Store>>> = Mutex::new(RefCell::new(None));

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
    catalog.clear_transient_active_slots();
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

pub fn usb_mode() -> GameLine {
    with_store(|store| store.catalog.usb_mode).unwrap_or(GameLine::Skylanders)
}

pub fn set_usb_mode_from_params(params: &str) -> Result<String, StorageError> {
    let mode = query_param(params, "mode")
        .or_else(|| query_param(params, "game"))
        .ok_or(StorageError::BadRequest)?;
    let mode = match mode.as_str() {
        "skylanders" => GameLine::Skylanders,
        "infinity" => GameLine::Infinity,
        _ => return Err(StorageError::BadRequest),
    };

    with_store_mut(|store| {
        let changed = store.catalog.usb_mode != mode;
        if changed {
            let generation = store.catalog.next_generation();
            store.catalog.usb_mode = mode;
            store.catalog.active_slots = [None; MAX_FIGURES];
            store.catalog.active_config_generation = generation;
            let payload = encode_config(store.catalog.active_slots, store.catalog.usb_mode);
            append_record(
                &mut store.flash,
                &mut store.catalog,
                RECORD_KIND_CONFIG_UPSERT,
                0,
                generation,
                &payload,
            )?;
        }

        Ok(format!(
            "{{\"mode\":\"{}\",\"changed\":{},\"reboot_required\":false,\"reenumerating\":{}}}\n",
            store.catalog.usb_mode.wire_name(),
            if changed { "true" } else { "false" },
            if changed { "true" } else { "false" }
        ))
    })
}

pub fn active_slot_images() -> Result<Vec<(u8, RecordId, Vec<u8>)>, StorageError> {
    active_slot_images_for_game(GameLine::Skylanders, MAX_FIGURES)
}

pub fn active_slot_images_for_game(
    game_line: GameLine,
    max_slots: usize,
) -> Result<Vec<(u8, RecordId, Vec<u8>)>, StorageError> {
    with_store_mut(|store| {
        let mut images = Vec::new();
        for slot in 0..MAX_FIGURES.min(max_slots) {
            let Some(id) = store.catalog.active_slots[slot] else {
                continue;
            };
            let entity = store.catalog.entity(id).ok_or(StorageError::NotFound)?;
            if entity.game_line != game_line {
                continue;
            }
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
        let entity_id = store.catalog.next_record_id();
        let image = initialize_new_entity_image(
            identity.kind,
            identity.character_id,
            identity.variant_id,
            entity_id.0,
        );
        let blob_id = append_blob(&mut store.flash, &mut store.catalog, &image)?;

        let image_crc32 = crc32(&image);
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
    let game = parse_game_param(params)?.unwrap_or(GameLine::Skylanders);
    let catalog_index = query_param(params, "catalog_index")
        .and_then(|value| parse_u32(value.as_str()))
        .and_then(|value| u16::try_from(value).ok())
        .ok_or(StorageError::BadRequest)?;
    let name = query_param(params, "name").ok_or(StorageError::BadRequest)?;
    match game {
        GameLine::Skylanders => create_skylanders_entity_from_catalog(catalog_index, &name),
        GameLine::Infinity => create_infinity_entity_from_catalog(catalog_index, &name),
    }
}

fn create_skylanders_entity_from_catalog(
    catalog_index: u16,
    name: &str,
) -> Result<String, StorageError> {
    let entry = skylanders_catalog_entry(catalog_index).ok_or(StorageError::NotFound)?;
    with_store_mut(|store| {
        let variant_id = if entry.has_variant() {
            Some(entry.variant_id)
        } else {
            None
        };
        let entity_id = store.catalog.next_record_id();
        let image =
            initialize_new_entity_image(entry.kind, entry.character_id, variant_id, entity_id.0);
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

fn create_infinity_entity_from_catalog(
    catalog_index: u16,
    name: &str,
) -> Result<String, StorageError> {
    let entry = infinity_catalog_entry(catalog_index).ok_or(StorageError::NotFound)?;
    with_store_mut(|store| {
        let entity_id = store.catalog.next_record_id();
        let image = initialize_infinity_entity_image(entry.figure_number, entity_id.0);
        let generation = store.catalog.next_generation();
        let entity = Entity {
            id: entity_id,
            name: FixedText::from_str(name).map_err(|_| StorageError::BadRequest)?,
            parent_identity_id: None,
            catalog_index: Some(entry.index),
            game_line: GameLine::Infinity,
            kind: entry.kind,
            data_mode: EntityDataMode::StaticGenerated,
            character_id: entry.figure_number,
            variant_id: None,
            blob_id: None,
            image_format: ImageFormat::InfinityUnknown,
            image_len: image.len() as u32,
            image_crc32: crc32(&image),
            created_generation: generation,
            updated_generation: generation,
        };
        append_entity_record(store, entity)?;
        Ok(format!(
            "{{\"created\":\"entity\",\"id\":{},\"catalog_index\":{},\"data_mode\":\"{}\",\"blob_id\":null,\"name\":\"{}\",\"figure\":\"{}\",\"game\":\"{}\"}}\n",
            entity_id.0,
            entry.index,
            entity.data_mode.wire_name(),
            json_escape(entity.name.as_str()),
            json_escape(entry.name),
            entity.game_line.wire_name()
        ))
    })
}

pub fn upload_entity_from_params(params: &str, image: &[u8]) -> Result<String, StorageError> {
    let game = parse_game_param(params)?.unwrap_or(GameLine::Skylanders);
    match game {
        GameLine::Skylanders => upload_skylanders_entity_from_params(params, image),
        GameLine::Infinity => upload_infinity_entity_from_params(params, image),
    }
}

pub fn upload_entity_from_form_params(params: &str) -> Result<String, StorageError> {
    let image_hex = query_param(params, "image_hex").ok_or(StorageError::BadRequest)?;
    let image = decode_hex_bytes(&image_hex).ok_or(StorageError::BadRequest)?;
    upload_entity_from_params(params, &image)
}

fn upload_skylanders_entity_from_params(
    params: &str,
    image: &[u8],
) -> Result<String, StorageError> {
    let name = query_param(params, "name").ok_or(StorageError::BadRequest)?;
    let identity_id =
        query_param(params, "identity_id").and_then(|value| parse_u32(value.as_str()));
    let imported = ImportedSkylandersImage::parse(image).ok_or(StorageError::BadRequest)?;

    with_store_mut(|store| {
        let identity = identity_id.and_then(|id| store.catalog.identity(RecordId(id)));
        if let Some(identity) = identity {
            if identity.character_id != imported.character_id
                || identity.variant_id != imported.variant_id
                || identity.image_format != ImageFormat::SkylandersMifare1k
            {
                return Err(StorageError::BadRequest);
            }
        }

        let blob_id = append_blob(&mut store.flash, &mut store.catalog, image)?;
        let image_crc32 = crc32(image);
        let entity_id = store.catalog.next_record_id();
        let generation = store.catalog.next_generation();
        let entity = Entity {
            id: entity_id,
            name: FixedText::from_str(&name).map_err(|_| StorageError::BadRequest)?,
            parent_identity_id: identity.map(|item| item.id),
            catalog_index: imported.catalog_entry.map(|entry| entry.index),
            game_line: GameLine::Skylanders,
            kind: imported
                .catalog_entry
                .map(|entry| entry.kind)
                .or_else(|| identity.map(|item| item.kind))
                .unwrap_or(FigureKind::Unknown),
            data_mode: EntityDataMode::MutableImage,
            character_id: imported.character_id,
            variant_id: imported.variant_id,
            blob_id: Some(blob_id),
            image_format: ImageFormat::SkylandersMifare1k,
            image_len: image.len() as u32,
            image_crc32,
            created_generation: generation,
            updated_generation: generation,
        };
        append_entity_record(store, entity)?;
        Ok(format!(
            "{{\"uploaded\":\"entity\",\"id\":{},\"blob_id\":{},\"name\":\"{}\",\"catalog_index\":{},\"character_id\":{},\"variant_id\":{},\"kind\":\"{}\",\"figure\":{}}}\n",
            entity_id.0,
            blob_id.0,
            json_escape(entity.name.as_str()),
            option_u16_json(entity.catalog_index),
            entity.character_id,
            option_u32_json(entity.variant_id),
            entity.kind.wire_name(),
            option_str_json(imported.catalog_entry.map(|entry| entry.name))
        ))
    })
}

fn upload_infinity_entity_from_params(params: &str, image: &[u8]) -> Result<String, StorageError> {
    let name = query_param(params, "name").ok_or(StorageError::BadRequest)?;
    let imported = ImportedInfinityImage::parse(image).ok_or(StorageError::BadRequest)?;
    let identity_id =
        query_param(params, "identity_id").and_then(|value| parse_u32(value.as_str()));

    with_store_mut(|store| {
        let identity = identity_id.and_then(|id| store.catalog.identity(RecordId(id)));
        let blob_id = append_blob(&mut store.flash, &mut store.catalog, image)?;
        let entity_id = store.catalog.next_record_id();
        let generation = store.catalog.next_generation();
        let entity = Entity {
            id: entity_id,
            name: FixedText::from_str(&name).map_err(|_| StorageError::BadRequest)?,
            parent_identity_id: identity.map(|item| item.id),
            catalog_index: imported.catalog_entry.map(|entry| entry.index),
            game_line: GameLine::Infinity,
            kind: identity
                .map(|item| item.kind)
                .or_else(|| imported.catalog_entry.map(|entry| entry.kind))
                .unwrap_or(FigureKind::Character),
            data_mode: EntityDataMode::MutableImage,
            character_id: identity
                .map(|item| item.character_id)
                .unwrap_or(imported.figure_number),
            variant_id: None,
            blob_id: Some(blob_id),
            image_format: ImageFormat::InfinityUnknown,
            image_len: image.len() as u32,
            image_crc32: crc32(image),
            created_generation: generation,
            updated_generation: generation,
        };
        append_entity_record(store, entity)?;
        Ok(format!(
            "{{\"uploaded\":\"entity\",\"id\":{},\"blob_id\":{},\"name\":\"{}\",\"game\":\"{}\",\"format\":\"{}\",\"image_len\":{},\"tag_id\":\"{}\"}}\n",
            entity_id.0,
            blob_id.0,
            json_escape(entity.name.as_str()),
            entity.game_line.wire_name(),
            entity.image_format.wire_name(),
            entity.image_len,
            hex_bytes(&imported.tag_id)
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
        let id = store.catalog.next_record_id();
        let mut image = read_entity_image(store, source)?;
        if source.image_format == ImageFormat::SkylandersMifare1k {
            rekey_skylanders_entity_image(&mut image, source.character_id, source.variant_id, id.0);
        }
        let image_crc32 = crc32(&image);
        let blob_id = append_blob(&mut store.flash, &mut store.catalog, &image)?;
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
            image_len: image.len() as u32,
            image_crc32,
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
    with_store_mut(|store| read_entity_blob_from_store(store, entity_id))
}

pub fn replace_entity_blob(entity_id: RecordId, image: &[u8]) -> Result<(), StorageError> {
    with_store_mut(|store| replace_entity_blob_in_store(store, entity_id, image))
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
        let entity = store.catalog.entity(id).ok_or(StorageError::NotFound)?;
        if !entity_can_use_active_slot(entity, slot as usize) {
            return Err(StorageError::BadRequest);
        }
        let generation = store.catalog.next_generation();
        store.catalog.place_entity_in_slot(id, slot as usize);
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
        compact_store(store)?;
        Ok(format!(
            "{{\"compacted\":true,\"used_bytes\":{}}}\n",
            store.catalog.write_offset
        ))
    })
}

fn ensure_journal_space(store: &mut Store, required_bytes: u32) -> Result<(), StorageError> {
    if journal_usage_percent(store.catalog.write_offset) >= PROACTIVE_COMPACT_USED_PERCENT {
        compact_store(store)?;
    }

    if store.catalog.write_offset + required_bytes <= config::STORAGE_FLASH_BYTES {
        return Ok(());
    }

    compact_store(store)?;
    if store.catalog.write_offset + required_bytes <= config::STORAGE_FLASH_BYTES {
        Ok(())
    } else {
        Err(StorageError::Full)
    }
}

fn compact_store(store: &mut Store) -> Result<(), StorageError> {
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
    let next_record_id = store.catalog.next_record_id;
    let next_blob_id = store.catalog.next_blob_id;
    let active_slots = store.catalog.active_slots;
    let usb_mode = store.catalog.usb_mode;
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

    store.catalog.active_slots = active_slots;
    store.catalog.usb_mode = usb_mode;
    store.catalog.active_config_generation = generation;
    let payload = encode_config(store.catalog.active_slots, store.catalog.usb_mode);
    append_record(
        &mut store.flash,
        &mut store.catalog,
        RECORD_KIND_CONFIG_UPSERT,
        0,
        generation,
        &payload,
    )?;
    store.catalog.next_generation = generation + 1;
    Ok(())
}

fn journal_record_len(payload_len: usize) -> u32 {
    align4(JOURNAL_RECORD_HEADER_BYTES as u32 + payload_len as u32)
}

fn journal_usage_percent(write_offset: u32) -> u32 {
    if config::STORAGE_FLASH_BYTES == 0 {
        return 100;
    }
    write_offset.saturating_mul(100) / config::STORAGE_FLASH_BYTES
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
        let generation = store.catalog.next_generation();
        append_record(
            &mut store.flash,
            &mut store.catalog,
            RECORD_KIND_FORMAT_MARKER,
            0,
            generation,
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

fn read_entity_blob_from_store(
    store: &mut Store,
    entity_id: RecordId,
) -> Result<Vec<u8>, StorageError> {
    let entity = store
        .catalog
        .entity(entity_id)
        .ok_or(StorageError::NotFound)?;
    read_entity_image(store, entity)
}

fn replace_entity_blob_in_store(
    store: &mut Store,
    entity_id: RecordId,
    image: &[u8],
) -> Result<(), StorageError> {
    let mut entity = store
        .catalog
        .entity(entity_id)
        .ok_or(StorageError::NotFound)?;
    if read_entity_image(store, entity)
        .map(|current| current == image)
        .unwrap_or(false)
    {
        return Ok(());
    }

    let required_bytes =
        journal_record_len(image.len()) + journal_record_len(core::mem::size_of::<[u8; 128]>());
    ensure_journal_space(store, required_bytes)?;

    let blob_id = append_blob(&mut store.flash, &mut store.catalog, image)?;
    entity.blob_id = Some(blob_id);
    entity.data_mode = EntityDataMode::MutableImage;
    entity.image_len = image.len() as u32;
    entity.image_crc32 = crc32(image);
    entity.updated_generation = store.catalog.next_generation();
    append_entity_record(store, entity)?;
    Ok(())
}

fn read_entity_image(store: &mut Store, entity: Entity) -> Result<Vec<u8>, StorageError> {
    let image = if let Some(blob_id) = entity.blob_id {
        read_blob(store, blob_id)?
    } else {
        generated_entity_image(entity)
    };
    Ok(image)
}

fn generated_entity_image(entity: Entity) -> Vec<u8> {
    match entity.game_line {
        GameLine::Skylanders => {
            let image = initialize_skylanders_entity_image(
                entity.character_id,
                entity.variant_id,
                entity.id.0,
            );
            let mut out = Vec::new();
            out.extend_from_slice(&image);
            out
        }
        GameLine::Infinity => {
            let image = initialize_infinity_entity_image(entity.character_id, entity.id.0);
            let mut out = Vec::new();
            out.extend_from_slice(&image);
            out
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ImportedSkylandersImage {
    character_id: u32,
    variant_id: Option<u32>,
    catalog_entry: Option<&'static SkylandersCatalogEntry>,
}

impl ImportedSkylandersImage {
    fn parse(image: &[u8]) -> Option<Self> {
        validate_skylanders_mifare_image(image).ok()?;
        let character_id = u16::from_le_bytes(image.get(0x10..0x12)?.try_into().ok()?) as u32;
        let raw_variant_id = u16::from_le_bytes(image.get(0x1c..0x1e)?.try_into().ok()?) as u32;
        let variant_id = if raw_variant_id == 0 {
            None
        } else {
            Some(raw_variant_id)
        };
        let catalog_entry = find_skylanders_catalog_entry(character_id, raw_variant_id);
        Some(Self {
            character_id,
            variant_id,
            catalog_entry,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ImportedInfinityImage {
    tag_id: [u8; 7],
    figure_number: u32,
    catalog_entry: Option<&'static crate::figures::infinity::FigureCatalogEntry>,
}

impl ImportedInfinityImage {
    fn parse(image: &[u8]) -> Option<Self> {
        if image.len() != INFINITY_IMAGE_BYTES {
            return None;
        }
        let tag_id = image.get(..7)?.try_into().ok()?;
        let image: &[u8; INFINITY_IMAGE_BYTES] = image.try_into().ok()?;
        let figure_number = infinity_figure_number(image);
        Some(Self {
            tag_id,
            figure_number,
            catalog_entry: find_infinity_catalog_entry(figure_number),
        })
    }
}

fn find_skylanders_catalog_entry(
    character_id: u32,
    raw_variant_id: u32,
) -> Option<&'static SkylandersCatalogEntry> {
    SKYLANDERS_CATALOG
        .iter()
        .find(|entry| entry.character_id == character_id && entry.variant_id == raw_variant_id)
}

fn initialize_new_entity_image(
    kind: FigureKind,
    character_id: u32,
    variant_id: Option<u32>,
    entity_id: u32,
) -> [u8; SKYLANDERS_IMAGE_BYTES] {
    if entity_kind_is_mutable(kind) {
        initialize_mutable_skylanders_entity_image(character_id, variant_id, entity_id, kind)
    } else {
        initialize_skylanders_entity_image(character_id, variant_id, entity_id)
    }
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

fn entity_can_use_active_slot(entity: Entity, slot: usize) -> bool {
    match entity.game_line {
        GameLine::Skylanders => slot < MAX_FIGURES,
        GameLine::Infinity => match entity.kind {
            FigureKind::Character | FigureKind::Unknown => slot < 2,
            _ => slot == 2,
        },
    }
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

#[cfg(test)]
fn append_config_record(
    flash: &mut StorageFlash,
    catalog: &mut Catalog,
    active_slots: [Option<RecordId>; MAX_FIGURES],
    usb_mode: GameLine,
    generation: u32,
) -> Result<(), StorageError> {
    let payload = encode_config(active_slots, usb_mode);
    append_record(
        flash,
        catalog,
        RECORD_KIND_CONFIG_UPSERT,
        0,
        generation,
        &payload,
    )
}

#[cfg(test)]
mod tests;
