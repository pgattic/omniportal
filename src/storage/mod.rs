use core::cell::RefCell;

use crate::config;
use crate::figures::formats::ImageFormat;
use crate::figures::init::initialize_skylanders_placeholder;
use crate::figures::{FigureKind, GameLine};
use crate::storage::records::{
    BackupBlob, BlobId, CharacterIdentity, CharacterInstance, FixedText, RecordId, StoredBlob,
    MAX_BACKUPS, MAX_IDENTITIES, MAX_INSTANCES, MAX_RECORD_NAME_BYTES,
};
use crate::storage::wear::{
    DEFAULT_COMMIT_DEBOUNCE_MS, JOURNAL_RECORD_HEADER_BYTES, JOURNAL_RECORD_MAGIC,
};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use critical_section::Mutex;
use embassy_time::{Duration, Timer};
use embedded_storage::nor_flash::{NorFlash, ReadNorFlash};
use esp_println::println;
use esp_storage::FlashStorage;

pub mod records;
pub mod wear;

const RECORD_KIND_IDENTITY_UPSERT: u8 = 1;
const RECORD_KIND_IDENTITY_DELETE: u8 = 2;
const RECORD_KIND_INSTANCE_UPSERT: u8 = 3;
const RECORD_KIND_INSTANCE_DELETE: u8 = 4;
const RECORD_KIND_BACKUP_UPSERT: u8 = 5;
const RECORD_KIND_BACKUP_DELETE: u8 = 6;
const RECORD_KIND_BLOB_DATA: u8 = 7;
const RECORD_KIND_FORMAT_MARKER: u8 = 254;

const ERASED_WORD: [u8; 4] = [0xff; 4];

static STORE: Mutex<RefCell<Option<Store>>> = Mutex::new(RefCell::new(None));

pub fn init() {
    let _ = DEFAULT_COMMIT_DEBOUNCE_MS;
    let mut flash = FlashStorage::new();
    let mut catalog = Catalog::new();
    let scan = scan_flash(&mut flash, &mut catalog);
    println!(
        "Storage scan: identities={}, instances={}, backups={}, used={} bytes, status={}",
        catalog.identity_count(),
        catalog.instance_count(),
        catalog.backup_count(),
        catalog.write_offset,
        if scan.is_ok() { "ok" } else { "needs-format" }
    );

    critical_section::with(|cs| {
        STORE.borrow_ref_mut(cs).replace(Store { flash, catalog });
    });
}

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

pub fn create_instance_from_query(query: &str) -> Result<String, StorageError> {
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
        let blob_id = store.catalog.next_blob_id();
        let blob_generation = store.catalog.next_generation();
        append_record(
            &mut store.flash,
            &mut store.catalog,
            RECORD_KIND_BLOB_DATA,
            blob_id.0,
            blob_generation,
            &image,
        )?;

        let image_crc32 = crc32(&image);
        let instance_id = store.catalog.next_record_id();
        let instance_generation = store.catalog.next_generation();
        let instance = CharacterInstance {
            id: instance_id,
            name: FixedText::from_str(&name).map_err(|_| StorageError::BadRequest)?,
            parent_identity_id: Some(identity.id),
            game_line: identity.game_line,
            blob_id,
            image_format: identity.image_format,
            image_len: image.len() as u32,
            image_crc32,
            created_generation: instance_generation,
            updated_generation: instance_generation,
        };
        let payload = encode_instance(&instance);
        append_record(
            &mut store.flash,
            &mut store.catalog,
            RECORD_KIND_INSTANCE_UPSERT,
            instance_id.0,
            instance_generation,
            &payload,
        )?;
        store.catalog.upsert_instance(instance)?;
        Ok(format!(
            "{{\"created\":\"instance\",\"id\":{},\"blob_id\":{},\"name\":\"{}\"}}\n",
            instance_id.0,
            blob_id.0,
            json_escape(instance.name.as_str())
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

pub fn delete_instance_from_query(query: &str) -> Result<String, StorageError> {
    delete_record_from_query(
        query,
        "instance",
        RECORD_KIND_INSTANCE_DELETE,
        |catalog, id| catalog.delete_instance(id),
    )
}

pub fn rename_instance_from_query(query: &str) -> Result<String, StorageError> {
    let id = query_param(query, "id")
        .and_then(|value| parse_u32(value.as_str()))
        .ok_or(StorageError::BadRequest)?;
    let name = query_param(query, "name").ok_or(StorageError::BadRequest)?;

    with_store_mut(|store| {
        let mut instance = store
            .catalog
            .instance(RecordId(id))
            .ok_or(StorageError::NotFound)?;
        instance.name = FixedText::from_str(&name).map_err(|_| StorageError::BadRequest)?;
        instance.updated_generation = store.catalog.next_generation();
        let payload = encode_instance(&instance);
        append_record(
            &mut store.flash,
            &mut store.catalog,
            RECORD_KIND_INSTANCE_UPSERT,
            id,
            instance.updated_generation,
            &payload,
        )?;
        store.catalog.upsert_instance(instance)?;
        Ok(format!("{{\"renamed\":\"instance\",\"id\":{}}}\n", id))
    })
}

pub fn read_instance_blob(instance_id: RecordId) -> Result<Vec<u8>, StorageError> {
    with_store_mut(|store| {
        let instance = store
            .catalog
            .instance(instance_id)
            .ok_or(StorageError::NotFound)?;
        read_blob(store, instance.blob_id)
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
    flash: FlashStorage,
    catalog: Catalog,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageError {
    Uninitialized,
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
            Self::Full => "507 Insufficient Storage",
            Self::Uninitialized | Self::Flash | Self::Corrupt => "500 Internal Server Error",
        }
    }

    pub const fn message(self) -> &'static str {
        match self {
            Self::Uninitialized => "storage uninitialized",
            Self::BadRequest => "bad request",
            Self::NotFound => "not found",
            Self::Full => "storage full",
            Self::Flash => "flash error",
            Self::Corrupt => "corrupt storage record",
        }
    }
}

#[derive(Clone, Copy)]
struct Catalog {
    identities: [Option<CharacterIdentity>; MAX_IDENTITIES],
    instances: [Option<CharacterInstance>; MAX_INSTANCES],
    backups: [Option<BackupBlob>; MAX_BACKUPS],
    blobs: [Option<StoredBlob>; MAX_INSTANCES + MAX_BACKUPS],
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
            instances: [None; MAX_INSTANCES],
            backups: [None; MAX_BACKUPS],
            blobs: [None; MAX_INSTANCES + MAX_BACKUPS],
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

    fn instance(&self, id: RecordId) -> Option<CharacterInstance> {
        self.instances
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

    fn upsert_instance(&mut self, instance: CharacterInstance) -> Result<(), StorageError> {
        upsert_by_id(&mut self.instances, instance, |item| item.id, instance.id)
    }

    fn upsert_backup(&mut self, backup: BackupBlob) -> Result<(), StorageError> {
        upsert_by_id(&mut self.backups, backup, |item| item.id, backup.id)
    }

    fn upsert_blob(&mut self, blob: StoredBlob) -> Result<(), StorageError> {
        upsert_by_id(&mut self.blobs, blob, |item| item.id, blob.id)
    }

    fn delete_identity(&mut self, id: RecordId) -> Result<(), StorageError> {
        delete_by_id(&mut self.identities, |item| item.id, id)
    }

    fn delete_instance(&mut self, id: RecordId) -> Result<(), StorageError> {
        delete_by_id(&mut self.instances, |item| item.id, id)
    }

    fn identity_count(&self) -> usize {
        self.identities.iter().filter(|item| item.is_some()).count()
    }

    fn instance_count(&self) -> usize {
        self.instances.iter().filter(|item| item.is_some()).count()
    }

    fn backup_count(&self) -> usize {
        self.backups.iter().filter(|item| item.is_some()).count()
    }

    fn status_json(&self) -> String {
        format!(
            "{{\"storage\":\"ok\",\"identities\":{},\"instances\":{},\"backups\":{},\"used_bytes\":{},\"capacity_bytes\":{},\"corrupt_records\":{}}}",
            self.identity_count(),
            self.instance_count(),
            self.backup_count(),
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
        out.push_str("],\"instances\":[");
        first = true;
        for instance in self.instances.iter().flatten() {
            if !first {
                out.push(',');
            }
            first = false;
            out.push_str(&format!(
                "{{\"id\":{},\"name\":\"{}\",\"identity_id\":{},\"game\":\"{}\",\"blob_id\":{},\"image_len\":{},\"crc32\":{}}}",
                instance.id.0,
                json_escape(instance.name.as_str()),
                instance.parent_identity_id.map(|id| id.0).unwrap_or(0),
                instance.game_line.wire_name(),
                instance.blob_id.0,
                instance.image_len,
                instance.image_crc32
            ));
        }
        out.push_str("],\"backups\":[]}\n");
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

fn scan_flash(flash: &mut FlashStorage, catalog: &mut Catalog) -> Result<(), StorageError> {
    let mut offset = 0;
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
            catalog.write_offset = align4(offset + 4);
            return Err(StorageError::Corrupt);
        }

        let mut header = [0; JOURNAL_RECORD_HEADER_BYTES];
        flash
            .read(config::STORAGE_FLASH_OFFSET + offset, &mut header)
            .map_err(|_| StorageError::Flash)?;
        let record = JournalHeader::decode(&header).ok_or(StorageError::Corrupt)?;
        let total_len = align4(JOURNAL_RECORD_HEADER_BYTES as u32 + record.payload_len);
        if offset + total_len > config::STORAGE_FLASH_BYTES {
            catalog.corrupt_records += 1;
            catalog.write_offset = offset;
            return Err(StorageError::Corrupt);
        }

        let payload_offset = offset + JOURNAL_RECORD_HEADER_BYTES as u32;
        let mut payload = Vec::new();
        payload.resize(record.payload_len as usize, 0);
        if !payload.is_empty() {
            flash
                .read(config::STORAGE_FLASH_OFFSET + payload_offset, &mut payload)
                .map_err(|_| StorageError::Flash)?;
        }
        if crc32(&payload) != record.payload_crc {
            catalog.corrupt_records += 1;
            catalog.write_offset = offset;
            return Err(StorageError::Corrupt);
        }

        apply_record(catalog, &record, payload_offset, &payload)?;
        offset += total_len;
        catalog.write_offset = offset;
    }
    Ok(())
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
        RECORD_KIND_INSTANCE_UPSERT => {
            if let Some(instance) = decode_instance(record.id, record.generation, payload) {
                catalog.observe_record_id(record.id, record.generation);
                catalog.upsert_instance(instance)?;
            }
        }
        RECORD_KIND_INSTANCE_DELETE => {
            catalog.observe_record_id(record.id, record.generation);
            let _ = catalog.delete_instance(RecordId(record.id));
        }
        RECORD_KIND_BACKUP_UPSERT => {
            if let Some(backup) = decode_backup(record.id, record.generation, payload) {
                catalog.observe_record_id(record.id, record.generation);
                catalog.upsert_backup(backup)?;
            }
        }
        RECORD_KIND_BACKUP_DELETE => {
            catalog.observe_record_id(record.id, record.generation);
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
        _ => {}
    }
    Ok(())
}

fn append_record(
    flash: &mut FlashStorage,
    catalog: &mut Catalog,
    kind: u8,
    id: u32,
    generation: u32,
    payload: &[u8],
) -> Result<(), StorageError> {
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
    catalog.write_offset += record.len() as u32;
    Ok(())
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

fn encode_instance(instance: &CharacterInstance) -> [u8; 96] {
    let mut out = [0; 96];
    out[0] = instance.game_line.as_u8();
    out[1] = instance.image_format.as_u8();
    out[2] = instance.name.len() as u8;
    out[3] = u8::from(instance.parent_identity_id.is_some());
    out[4..8].copy_from_slice(
        &instance
            .parent_identity_id
            .map(|id| id.0)
            .unwrap_or(0)
            .to_le_bytes(),
    );
    out[8..12].copy_from_slice(&instance.blob_id.0.to_le_bytes());
    out[12..16].copy_from_slice(&instance.image_len.to_le_bytes());
    out[16..20].copy_from_slice(&instance.image_crc32.to_le_bytes());
    out[20..24].copy_from_slice(&instance.created_generation.to_le_bytes());
    out[24..28].copy_from_slice(&instance.updated_generation.to_le_bytes());
    out[32..32 + instance.name.len()].copy_from_slice(instance.name.raw_bytes());
    out
}

fn decode_instance(id: u32, _generation: u32, payload: &[u8]) -> Option<CharacterInstance> {
    if payload.len() < 96 {
        return None;
    }
    let name_len = payload[2] as usize;
    if name_len > MAX_RECORD_NAME_BYTES || 32 + name_len > payload.len() {
        return None;
    }
    let name = core::str::from_utf8(&payload[32..32 + name_len]).ok()?;
    Some(CharacterInstance {
        id: RecordId(id),
        game_line: GameLine::from_u8(payload[0])?,
        image_format: ImageFormat::from_u8(payload[1])?,
        name: FixedText::from_str(name).ok()?,
        parent_identity_id: if payload[3] == 1 {
            Some(RecordId(u32::from_le_bytes(payload[4..8].try_into().ok()?)))
        } else {
            None
        },
        blob_id: BlobId(u32::from_le_bytes(payload[8..12].try_into().ok()?)),
        image_len: u32::from_le_bytes(payload[12..16].try_into().ok()?),
        image_crc32: u32::from_le_bytes(payload[16..20].try_into().ok()?),
        created_generation: u32::from_le_bytes(payload[20..24].try_into().ok()?),
        updated_generation: u32::from_le_bytes(payload[24..28].try_into().ok()?),
    })
}

fn decode_backup(_id: u32, _generation: u32, _payload: &[u8]) -> Option<BackupBlob> {
    None
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
        let (key, value) = pair.split_once('=')?;
        if key == name {
            return Some(percent_decode(value));
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
