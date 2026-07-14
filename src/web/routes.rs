pub const STATUS_PATH: &str = "/status";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Route {
    Index,
    Status,
}
