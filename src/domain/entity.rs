#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GameLine {
    Skylanders,
    Infinity,
}

impl GameLine {
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::Skylanders => 1,
            Self::Infinity => 2,
        }
    }

    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Skylanders),
            2 => Some(Self::Infinity),
            _ => None,
        }
    }

    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::Skylanders => "skylanders",
            Self::Infinity => "infinity",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FigureKind {
    Character,
    Vehicle,
    Item,
    Trap,
    CreationCrystal,
    LevelPiece,
    Trophy,
    PowerDisc,
    Unknown,
}

impl FigureKind {
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::Character => 1,
            Self::Vehicle => 2,
            Self::Item => 3,
            Self::Trap => 4,
            Self::CreationCrystal => 5,
            Self::LevelPiece => 6,
            Self::Trophy => 7,
            Self::PowerDisc => 8,
            Self::Unknown => 255,
        }
    }

    pub const fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Character,
            2 => Self::Vehicle,
            3 => Self::Item,
            4 => Self::Trap,
            5 => Self::CreationCrystal,
            6 => Self::LevelPiece,
            7 => Self::Trophy,
            8 => Self::PowerDisc,
            _ => Self::Unknown,
        }
    }

    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::Character => "character",
            Self::Vehicle => "vehicle",
            Self::Item => "item",
            Self::Trap => "trap",
            Self::CreationCrystal => "creation-crystal",
            Self::LevelPiece => "level-piece",
            Self::Trophy => "trophy",
            Self::PowerDisc => "power-disc",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageFormat {
    SkylandersMifare1k,
    InfinityUnknown,
}

impl ImageFormat {
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::SkylandersMifare1k => 1,
            Self::InfinityUnknown => 2,
        }
    }

    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::SkylandersMifare1k),
            2 => Some(Self::InfinityUnknown),
            _ => None,
        }
    }

    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::SkylandersMifare1k => "skylanders-mifare-1k",
            Self::InfinityUnknown => "infinity-unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CollectionEntity {
    pub id: u32,
    pub game_line: GameLine,
    pub payload: EntityPayload,
    pub blob_id: Option<u32>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntityPayload {
    Skylanders(SkylandersEntity),
    Infinity(InfinityEntity),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SkylandersEntity {
    pub catalog_index: Option<u16>,
    pub figure_id: u16,
    pub variant_id: Option<u16>,
    pub kind: FigureKind,
    pub image_format: ImageFormat,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InfinityEntity {
    pub catalog_index: Option<u16>,
    pub figure_number: u32,
    pub kind: FigureKind,
    pub image_format: ImageFormat,
}
