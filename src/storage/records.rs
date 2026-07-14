use crate::figures::formats::ImageFormat;
use crate::figures::{FigureKind, GameLine};

pub const MAX_RECORD_NAME_BYTES: usize = 64;
pub const MAX_SOURCE_NOTES_BYTES: usize = 96;
pub const MAX_IDENTITIES: usize = 32;
pub const MAX_INSTANCES: usize = 64;
pub const MAX_BACKUPS: usize = 16;

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
pub struct CharacterInstance {
    pub id: RecordId,
    pub name: FixedText<MAX_RECORD_NAME_BYTES>,
    pub parent_identity_id: Option<RecordId>,
    pub game_line: GameLine,
    pub blob_id: BlobId,
    pub image_format: ImageFormat,
    pub image_len: u32,
    pub image_crc32: u32,
    pub created_generation: u32,
    pub updated_generation: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackupBlob {
    pub id: RecordId,
    pub name: FixedText<MAX_RECORD_NAME_BYTES>,
    pub game_line: Option<GameLine>,
    pub blob_id: BlobId,
    pub image_format: ImageFormat,
    pub image_len: u32,
    pub image_crc32: u32,
    pub source_notes: FixedText<MAX_SOURCE_NOTES_BYTES>,
    pub generation: u32,
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
