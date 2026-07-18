use crate::domain::{FigureKind, GameLine};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FigureCatalogEntry {
    pub index: u16,
    pub game_line: GameLine,
    pub kind: FigureKind,
    pub series: &'static str,
    pub name: &'static str,
    pub figure_number: u32,
}

pub const INFINITY_CATALOG: &[FigureCatalogEntry] = &[];
