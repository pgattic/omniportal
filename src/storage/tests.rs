use super::journal::*;
use super::*;
use crate::figures::skylanders::crypto::decrypt_figure;
use crate::storage::wear::JOURNAL_RECORD_MAGIC;
use crate::usb::skylanders::{handle_command, PortalState, PLACEMENT_STATUS_HOLD_REPORTS};

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
fn parses_game_params_for_catalog_and_import_dispatch() {
    assert_eq!(parse_game_param(""), Ok(None));
    assert_eq!(parse_game_param("game="), Ok(None));
    assert_eq!(
        parse_game_param("game=skylanders"),
        Ok(Some(GameLine::Skylanders))
    );
    assert_eq!(
        parse_game_param("game=infinity"),
        Ok(Some(GameLine::Infinity))
    );
    assert_eq!(
        parse_game_param("game=unknown"),
        Err(StorageError::BadRequest)
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
    assert_eq!(decoded.usb_mode, GameLine::Skylanders);
    assert_eq!(decoded.active_slots[0], Some(RecordId(42)));
    assert_eq!(decoded.active_slots[1], None);

    let mut active_slots = [None; MAX_FIGURES];
    active_slots[0] = Some(RecordId(8));
    active_slots[3] = Some(RecordId(11));

    let decoded = decode_config(&encode_config(active_slots, GameLine::Infinity));
    assert_eq!(decoded.usb_mode, GameLine::Infinity);
    assert_eq!(decoded.active_slots[0], Some(RecordId(8)));
    assert_eq!(decoded.active_slots[3], Some(RecordId(11)));
    assert_eq!(decoded.active_slots[4], None);
}

#[test]
fn selecting_entity_moves_it_between_slots() {
    let mut catalog = Catalog::new();
    catalog.place_entity_in_slot(RecordId(1), 0);
    catalog.place_entity_in_slot(RecordId(2), 1);
    catalog.place_entity_in_slot(RecordId(1), 2);

    assert_eq!(catalog.active_slots[0], None);
    assert_eq!(catalog.active_slots[1], Some(RecordId(2)));
    assert_eq!(catalog.active_slots[2], Some(RecordId(1)));
}

#[test]
fn infinity_entities_are_limited_to_compatible_active_slots() {
    let mut character = test_entity();
    character.game_line = GameLine::Infinity;
    character.kind = FigureKind::Character;

    assert!(entity_can_use_active_slot(character, 0));
    assert!(entity_can_use_active_slot(character, 1));
    assert!(!entity_can_use_active_slot(character, 2));

    let mut disc = character;
    disc.kind = FigureKind::PowerDisc;
    assert!(!entity_can_use_active_slot(disc, 0));
    assert!(!entity_can_use_active_slot(disc, 1));
    assert!(entity_can_use_active_slot(disc, 2));
}

#[test]
fn new_mutable_entity_images_include_encrypted_fresh_save_data() {
    let image = initialize_new_entity_image(FigureKind::Character, 21, None, 1);
    let plaintext = decrypt_figure(&image);

    assert_ne!(image, plaintext);
    assert_eq!(&plaintext[0x10..0x12], &21u16.to_le_bytes());
    assert_eq!(&plaintext[0x80 + 0x5a..0x80 + 0x5c], &1u16.to_le_bytes());
}

#[test]
fn skylanders_character_like_subtypes_initialize_like_characters() {
    for kind in [
        FigureKind::Giant,
        FigureKind::Swapper,
        FigureKind::TrapMaster,
        FigureKind::Mini,
        FigureKind::CreationCrystal,
    ] {
        let image = initialize_new_entity_image(kind, 21, None, 1);
        let plaintext = decrypt_figure(&image);

        assert_ne!(image, plaintext);
        assert_eq!(&plaintext[0x80 + 0x5a..0x80 + 0x5c], &1u16.to_le_bytes());
    }
}

#[test]
fn skylanders_unknown_layout_toys_keep_plain_dolphin_create_layout() {
    for kind in [FigureKind::Trap, FigureKind::Vehicle] {
        let image = initialize_new_entity_image(kind, 214, Some(0x3001), 1);

        assert_eq!(decrypt_figure(&image), image);
        assert_eq!(&image[0x10..0x12], &214u16.to_le_bytes());
        assert_eq!(&image[0x1c..0x1e], &0x3001u16.to_le_bytes());
    }
}

#[test]
fn new_static_entity_images_keep_dolphin_create_layout_without_save_blob() {
    let image = initialize_new_entity_image(FigureKind::Item, 230, None, 1);

    assert_eq!(decrypt_figure(&image), image);
    assert_eq!(&image[0x10..0x12], &230u16.to_le_bytes());
}

#[test]
fn imported_skylanders_images_are_validated_and_inferred() {
    let image = initialize_new_entity_image(FigureKind::Character, 19, None, 1);
    let imported = ImportedSkylandersImage::parse(&image).unwrap();

    assert_eq!(imported.character_id, 19);
    assert_eq!(imported.variant_id, None);
    assert_eq!(imported.catalog_entry.unwrap().name, "Trigger Happy");

    let mut bad = image;
    bad[4] ^= 0x01;
    assert_eq!(ImportedSkylandersImage::parse(&bad), None);
}

#[test]
fn imported_infinity_images_require_raw_figure_size_and_expose_tag_id() {
    let image = initialize_infinity_entity_image(0x0f4241, 123);

    let imported = ImportedInfinityImage::parse(&image).unwrap();
    assert_eq!(imported.tag_id, image[..7]);
    assert_eq!(imported.figure_number, 0x0f4241);
    assert_eq!(imported.catalog_entry.unwrap().name, "Mr. Incredible");
    assert_eq!(hex_bytes(&imported.tag_id).len(), 14);

    assert_eq!(ImportedInfinityImage::parse(&image[..319]), None);
    let mut oversized = Vec::new();
    oversized.resize(INFINITY_IMAGE_BYTES + 1, 0);
    assert_eq!(ImportedInfinityImage::parse(&oversized), None);
}

#[test]
fn decodes_form_hex_image_payload() {
    let mut image = [0u8; INFINITY_IMAGE_BYTES];
    image[..7].copy_from_slice(&[0x04, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06]);
    let params = format!(
        "game=infinity&name=Mr+Incredible&image_hex={}",
        hex_bytes(&image)
    );
    let image_hex = query_param(&params, "image_hex").unwrap();
    let decoded = decode_hex_bytes(&image_hex).unwrap();

    assert_eq!(decoded, image);
    assert_eq!(
        ImportedInfinityImage::parse(&decoded).unwrap().tag_id,
        image[..7]
    );
    assert_eq!(decode_hex_bytes("abc"), None);
    assert_eq!(decode_hex_bytes("zz"), None);
}

#[test]
fn write_back_loop_exports_replaced_entity_image() {
    let entity_id = RecordId(8);
    let (mut store, image) = store_with_mutable_entity(entity_id);

    let mut portal = PortalState::new();
    assert!(portal.load_entity_into_slot(0, entity_id.0, &image));
    for _ in 0..=PLACEMENT_STATUS_HOLD_REPORTS {
        portal.next_status_report();
    }
    let mut write_command = [0; 19];
    write_command[0] = b'W';
    write_command[1] = 0x10;
    write_command[2] = 0x02;
    write_command[3..].copy_from_slice(&[0xa5; 16]);
    assert_eq!(
        &handle_command(&mut portal, &write_command).unwrap().report[..3],
        &[b'W', 0x10, 0x02]
    );

    let changed = portal.slot_image(0).unwrap();
    replace_entity_blob_in_store(&mut store, entity_id, changed).unwrap();

    assert_eq!(
        read_entity_blob_from_store(&mut store, entity_id).unwrap(),
        changed
    );
    let updated = store.catalog.entity(entity_id).unwrap();
    assert_eq!(updated.image_len, changed.len() as u32);
    assert_eq!(updated.image_crc32, crc32(changed));
}

#[test]
fn replacing_entity_with_unchanged_image_does_not_append_records() {
    let entity_id = RecordId(8);
    let (mut store, image) = store_with_mutable_entity(entity_id);
    let used_before = store.catalog.write_offset;

    replace_entity_blob_in_store(&mut store, entity_id, &image).unwrap();

    assert_eq!(store.catalog.write_offset, used_before);
}

#[test]
fn replacing_entity_blob_auto_compacts_when_journal_is_full() {
    let entity_id = RecordId(8);
    let (mut store, image) = full_store_with_mutable_entity(entity_id);
    store.catalog.active_slots[0] = Some(entity_id);
    store.catalog.usb_mode = GameLine::Infinity;

    let used_before = store.catalog.write_offset;
    let mut changed = image;
    changed[0x20] ^= 0x5a;
    replace_entity_blob_in_store(&mut store, entity_id, &changed).unwrap();

    assert!(store.catalog.write_offset < used_before);
    assert_eq!(store.catalog.active_slots[0], Some(entity_id));
    assert_eq!(store.catalog.usb_mode, GameLine::Infinity);
    assert_eq!(
        read_entity_blob_from_store(&mut store, entity_id).unwrap(),
        changed
    );
}

#[test]
fn replacing_entity_blob_proactively_compacts_above_usage_threshold() {
    let entity_id = RecordId(8);
    let (mut store, image) = store_with_mutable_entity(entity_id);
    store.catalog.active_slots[0] = Some(entity_id);
    let mut stale = [0x44; 1024];
    let stale_record_len = journal_record_len(stale.len());
    let target = crate::config::STORAGE_FLASH_BYTES
        * crate::storage::wear::PROACTIVE_COMPACT_USED_PERCENT
        / 100;
    let mut record_id = 20_000;

    while store.catalog.write_offset < target
        && store.catalog.write_offset + stale_record_len < crate::config::STORAGE_FLASH_BYTES
    {
        append_record(
            &mut store.flash,
            &mut store.catalog,
            RECORD_KIND_BLOB_DATA,
            record_id,
            record_id,
            &stale,
        )
        .unwrap();
        stale[0] = stale[0].wrapping_add(1);
        record_id += 1;
    }
    assert!(
        journal_usage_percent(store.catalog.write_offset)
            >= crate::storage::wear::PROACTIVE_COMPACT_USED_PERCENT
    );

    let used_before = store.catalog.write_offset;
    let mut changed = image;
    changed[0x20] ^= 0x33;
    replace_entity_blob_in_store(&mut store, entity_id, &changed).unwrap();

    assert!(store.catalog.write_offset < used_before);
    assert_eq!(store.catalog.active_slots[0], Some(entity_id));
    assert_eq!(
        read_entity_blob_from_store(&mut store, entity_id).unwrap(),
        changed
    );
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

fn store_with_mutable_entity(entity_id: RecordId) -> (Store, Vec<u8>) {
    let mut store = Store {
        flash: StorageFlash::new(),
        catalog: Catalog::new(),
    };
    append_record(
        &mut store.flash,
        &mut store.catalog,
        RECORD_KIND_FORMAT_MARKER,
        0,
        1,
        b"omniportal-storage-v1",
    )
    .unwrap();

    let image = initialize_new_entity_image(FigureKind::Character, 19, None, entity_id.0);
    let blob_id = append_blob(&mut store.flash, &mut store.catalog, &image).unwrap();
    let mut entity = test_entity();
    entity.id = entity_id;
    entity.character_id = 19;
    entity.variant_id = None;
    entity.catalog_index = Some(67);
    entity.blob_id = Some(blob_id);
    entity.image_crc32 = crc32(&image);
    append_entity_record(&mut store, entity).unwrap();

    (store, image.to_vec())
}

fn full_store_with_mutable_entity(entity_id: RecordId) -> (Store, Vec<u8>) {
    let (mut store, image) = store_with_mutable_entity(entity_id);
    let mut stale = [0x55; 1024];
    let required = journal_record_len(image.len()) + journal_record_len(128);
    let stale_record_len = journal_record_len(stale.len());
    let mut record_id = 10_000;

    while store.catalog.write_offset + stale_record_len <= crate::config::STORAGE_FLASH_BYTES {
        append_record(
            &mut store.flash,
            &mut store.catalog,
            RECORD_KIND_BLOB_DATA,
            record_id,
            record_id,
            &stale,
        )
        .unwrap();
        stale[0] = stale[0].wrapping_add(1);
        record_id += 1;
    }
    while store.catalog.write_offset + journal_record_len(0) <= crate::config::STORAGE_FLASH_BYTES {
        append_record(
            &mut store.flash,
            &mut store.catalog,
            RECORD_KIND_ENTITY_DELETE,
            record_id,
            record_id,
            &[],
        )
        .unwrap();
        record_id += 1;
    }
    assert!(store.catalog.write_offset + required > crate::config::STORAGE_FLASH_BYTES);

    (store, image)
}

#[test]
fn library_json_resolves_catalog_names_by_game_line() {
    let mut catalog = Catalog::new();
    let mut entity = test_entity();
    entity.game_line = GameLine::Infinity;
    entity.catalog_index = Some(2);
    entity.character_id = 0x0f4243;
    entity.variant_id = None;
    entity.blob_id = None;
    entity.image_format = ImageFormat::InfinityUnknown;
    catalog.upsert_entity(entity).unwrap();

    let json = catalog.library_json();
    assert!(json.contains("\"figure\":\"Jack Sparrow\""));
    assert!(json.contains("\"theme\":\"pirates-of-the-caribbean\""));
    assert!(json.contains("\"theme_label\":\"Pirates of the Caribbean\""));
    assert!(!json.contains("Polar Whirlwind"));
}

#[test]
fn storage_entities_project_to_game_specific_domain_payloads() {
    let skylanders = test_entity().domain_entity();
    let crate::domain::EntityPayload::Skylanders(payload) = skylanders.payload else {
        panic!("expected skylanders payload");
    };
    assert_eq!(skylanders.id, 8);
    assert_eq!(payload.figure_id, 21);
    assert_eq!(payload.variant_id, Some(3));

    let mut infinity = test_entity();
    infinity.game_line = GameLine::Infinity;
    infinity.character_id = 0x1234_5678;
    infinity.variant_id = None;
    infinity.image_format = ImageFormat::InfinityUnknown;
    let infinity = infinity.domain_entity();
    let crate::domain::EntityPayload::Infinity(payload) = infinity.payload else {
        panic!("expected infinity payload");
    };
    assert_eq!(payload.figure_number, 0x1234_5678);
    assert_eq!(payload.kind, FigureKind::Character);
    assert_eq!(payload.image_format, ImageFormat::InfinityUnknown);
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
    append_config_record(
        &mut flash,
        &mut catalog,
        active_slots,
        GameLine::Infinity,
        5,
    )
    .unwrap();

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
    assert_eq!(rebuilt.usb_mode, GameLine::Infinity);
    assert_eq!(rebuilt.next_record_id, 9);
    assert_eq!(rebuilt.next_blob_id, 3);
    assert_eq!(rebuilt.next_generation, 6);
}

#[test]
fn formatted_storage_resets_usb_mode_to_factory_default() {
    let mut flash = StorageFlash::new();
    let mut catalog = Catalog::new();
    catalog.usb_mode = GameLine::Infinity;
    let format_generation = catalog.next_generation();
    append_record(
        &mut flash,
        &mut catalog,
        RECORD_KIND_FORMAT_MARKER,
        0,
        format_generation,
        b"omniportal-storage-v1",
    )
    .unwrap();
    let mut rebuilt = Catalog::new();
    scan_flash(&mut flash, &mut rebuilt).unwrap();

    assert_eq!(rebuilt.usb_mode, GameLine::Skylanders);
    assert_eq!(rebuilt.active_slots, [None; MAX_FIGURES]);
    assert_eq!(rebuilt.next_generation, 2);
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
    flash.bytes[torn_offset..torn_offset + 4].copy_from_slice(&JOURNAL_RECORD_MAGIC.to_le_bytes());

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
