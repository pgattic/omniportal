pub const STATUS_PATH: &str = "/status";
pub const BAD_REQUEST_RESPONSE: &[u8] =
    b"HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nBad request\n";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Route {
    Index,
    Status,
}

pub fn request_target(request: &[u8]) -> Option<&str> {
    let request = core::str::from_utf8(request).ok()?;
    let first_line = request.lines().next()?;
    let mut parts = first_line.split_whitespace();
    let _method = parts.next()?;
    parts.next()
}
