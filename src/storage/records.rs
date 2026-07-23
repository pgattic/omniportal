use crate::domain::{
    CollectionEntity, EntityPayload, FigureKind, GameLine, ImageFormat, InfinityEntity,
    SkylandersEntity,
};

pub const MAX_RECORD_NAME_BYTES: usize = 64;
pub const MAX_SOURCE_NOTES_BYTES: usize = 96;
pub const MAX_IDENTITIES: usize = 32;
pub const MAX_ENTITIES: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecordId(pub u32);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BlobId(pub u32);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FixedText<const N: usize> {
    bytes: [u8; N],
    len: u8,
}

impl<const N: usize> FixedText<N> {
    pub const fn empty() -> Self {
        Self {
            bytes: [0; N],
            len: 0,
        }
    }

    pub fn from_str(value: &str) -> Result<Self, StorageRecordError> {
        let raw = value.as_bytes();
        if raw.is_empty() || raw.len() > N || raw.len() > u8::MAX as usize {
            return Err(StorageRecordError::InvalidText);
        }

        let mut text = Self::empty();
        text.bytes[..raw.len()].copy_from_slice(raw);
        text.len = raw.len() as u8;
        Ok(text)
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or("")
    }

    pub const fn len(&self) -> usize {
        self.len as usize
    }

    pub fn raw_bytes(&self) -> &[u8] {
        &self.bytes[..self.len()]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CharacterIdentity {
    pub id: RecordId,
    pub game_line: GameLine,
    pub name: FixedText<MAX_RECORD_NAME_BYTES>,
    pub character_id: u32,
    pub variant_id: Option<u32>,
    pub kind: FigureKind,
    pub image_format: ImageFormat,
    pub source_notes: FixedText<MAX_SOURCE_NOTES_BYTES>,
    pub generation: u32,
    pub checksum: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntityDataMode {
    StaticGenerated,
    MutableImage,
}

impl EntityDataMode {
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::StaticGenerated => 1,
            Self::MutableImage => 2,
        }
    }

    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::StaticGenerated),
            2 => Some(Self::MutableImage),
            _ => None,
        }
    }

    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::StaticGenerated => "static-generated",
            Self::MutableImage => "mutable-image",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Entity {
    pub id: RecordId,
    pub name: FixedText<MAX_RECORD_NAME_BYTES>,
    pub parent_identity_id: Option<RecordId>,
    pub catalog_index: Option<u16>,
    pub game_line: GameLine,
    pub kind: FigureKind,
    pub data_mode: EntityDataMode,
    pub character_id: u32,
    pub variant_id: Option<u32>,
    pub blob_id: Option<BlobId>,
    pub image_format: ImageFormat,
    pub image_len: u32,
    pub image_crc32: u32,
    pub created_generation: u32,
    pub updated_generation: u32,
    pub swapper_top_entity_id: Option<RecordId>,
    pub swapper_bottom_entity_id: Option<RecordId>,
}

impl Entity {
    pub fn is_swapper_combo(self) -> bool {
        self.kind == FigureKind::Swapper
            && self.swapper_top_entity_id.is_some()
            && self.swapper_bottom_entity_id.is_some()
    }

    pub fn domain_entity(self) -> CollectionEntity {
        CollectionEntity {
            id: self.id.0,
            game_line: self.game_line,
            payload: match self.game_line {
                GameLine::Skylanders => EntityPayload::Skylanders(SkylandersEntity {
                    catalog_index: self.catalog_index,
                    figure_id: self.character_id as u16,
                    variant_id: self.variant_id.map(|value| value as u16),
                    kind: self.kind,
                    image_format: self.image_format,
                }),
                GameLine::Infinity => EntityPayload::Infinity(InfinityEntity {
                    catalog_index: self.catalog_index,
                    figure_number: self.character_id,
                    kind: self.kind,
                    image_format: self.image_format,
                }),
            },
            blob_id: self.blob_id.map(|id| id.0),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StoredBlob {
    pub id: BlobId,
    pub offset: u32,
    pub len: u32,
    pub crc32: u32,
    pub generation: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageRecordError {
    InvalidText,
}
