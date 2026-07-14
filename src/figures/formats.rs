pub const SKYLANDERS_IMAGE_BYTES: usize = 1024;
pub const MAX_FIGURE_IMAGE_BYTES: usize = SKYLANDERS_IMAGE_BYTES;

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
