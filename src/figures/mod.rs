pub mod formats;
pub mod init;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GameLine {
    Skylanders,
    Infinity,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FigureIdentity {
    pub game_line: GameLine,
    pub model_id: u32,
}

pub fn initialize() {
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
