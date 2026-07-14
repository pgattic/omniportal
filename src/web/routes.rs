pub const STATUS_PATH: &str = "/status";
pub const STATUS_RESPONSE: &[u8] = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"mode\":\"skylanders\",\"active_instance\":null,\"storage\":\"stub\"}\n";
pub const BAD_REQUEST_RESPONSE: &[u8] =
    b"HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nBad request\n";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Route {
    Index,
    Status,
}

pub fn is_status_request(request: &[u8]) -> bool {
    request.starts_with(b"GET /status ")
}
