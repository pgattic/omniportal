#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PortalMode {
    Skylanders,
    Infinity,
}

pub const SUPPORTED_MODES: [PortalMode; 2] = [PortalMode::Skylanders, PortalMode::Infinity];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AppState {
    pub mode: PortalMode,
    pub active_entity_id: Option<u32>,
}

impl AppState {
    pub const fn new() -> Self {
        Self {
            mode: PortalMode::Skylanders,
            active_entity_id: None,
        }
    }
}
