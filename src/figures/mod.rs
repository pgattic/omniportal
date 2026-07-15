pub mod catalog;
pub mod formats;
pub mod init;

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
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FigureIdentity {
    pub game_line: GameLine,
    pub model_id: u32,
}

pub fn initialize() {
    let _ = catalog::SKYLANDERS_CATALOG.len();
    let _ = formats::MAX_FIGURE_IMAGE_BYTES;
    let _ = init::DEFAULT_INSTANCE_NAME;
    let _ = FigureIdentity {
        game_line: GameLine::Skylanders,
        model_id: 0,
    };
    let _ = FigureIdentity {
        game_line: GameLine::Infinity,
        model_id: 0,
    };
}
