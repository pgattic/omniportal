use alloc::{format, string::String, vec::Vec};

use crate::web::{http, routes, ui_html};

pub enum Response {
    Text {
        status: &'static str,
        content_type: &'static str,
        body: String,
    },
    StaticText {
        status: &'static str,
        content_type: &'static str,
        body: &'static str,
    },
    Binary {
        status: &'static str,
        content_type: &'static str,
        body: Vec<u8>,
    },
    Raw(&'static [u8]),
}

impl Response {
    pub fn bad_request() -> Self {
        Self::Raw(routes::BAD_REQUEST_RESPONSE)
    }
}

pub fn handle_request(request: &[u8]) -> Response {
    let method = routes::request_method(request).unwrap_or("");
    let Some(target) = routes::request_target(request) else {
        return Response::bad_request();
    };
    let (path, query) = http::split_target(target);
    let body = http::request_body(request).unwrap_or(&[]);

    if method == "GET" && path == "/" {
        Response::StaticText {
            status: "200 OK",
            content_type: "text/html",
            body: ui_html::INDEX_HTML,
        }
    } else if method == "GET" && path == "/favicon.ico" {
        Response::Raw(b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n")
    } else if method == "GET" && path == routes::STATUS_PATH {
        Response::Text {
            status: "200 OK",
            content_type: "application/json",
            body: format!(
                "{{\"mode\":\"{}\",\"active_entity\":{},\"active_slots\":{},\"storage\":{}}}\n",
                crate::storage::usb_mode().wire_name(),
                crate::storage::active_entity_json(),
                crate::storage::active_slots_json(),
                crate::storage::status_json()
            ),
        }
    } else if method == "GET" && path == "/api/library" {
        Response::Text {
            status: "200 OK",
            content_type: "application/json",
            body: crate::storage::library_json(),
        }
    } else if method == "GET" && path == "/api/catalog" {
        catalog_response(query)
    } else if method == "POST" && path == "/api/mode/set" {
        let before_mode = crate::storage::usb_mode();
        let result = crate::storage::set_usb_mode_from_params(http::params(query, body).as_str());
        if result.is_ok() && crate::storage::usb_mode() != before_mode {
            request_mode_reenumeration();
        }
        storage_result(result)
    } else if method == "POST" && path == "/api/identity/create" {
        storage_result(crate::storage::create_identity_from_params(
            http::params(query, body).as_str(),
        ))
    } else if method == "POST" && path == "/api/entity/create" {
        storage_result(crate::storage::create_entity_from_params(
            http::params(query, body).as_str(),
        ))
    } else if method == "POST" && path == "/api/entity/create-from-catalog" {
        storage_result(crate::storage::create_entity_from_catalog_params(
            http::params(query, body).as_str(),
        ))
    } else if method == "POST" && path == "/api/entity/upload" {
        let result = if query.is_empty() {
            crate::storage::upload_entity_from_form_params(http::params(query, body).as_str())
        } else {
            crate::storage::upload_entity_from_params(query, body)
        };
        storage_result(result)
    } else if method == "POST" && path == "/api/entity/clone" {
        storage_result(crate::storage::clone_entity_from_params(
            http::params(query, body).as_str(),
        ))
    } else if method == "POST" && path == "/api/identity/delete" {
        storage_result(crate::storage::delete_identity_from_query(
            http::params(query, body).as_str(),
        ))
    } else if method == "POST" && path == "/api/identity/rename" {
        storage_result(crate::storage::rename_identity_from_query(
            http::params(query, body).as_str(),
        ))
    } else if method == "POST" && path == "/api/entity/delete" {
        storage_result(crate::storage::delete_entity_from_query(
            http::params(query, body).as_str(),
        ))
    } else if method == "POST" && path == "/api/entity/rename" {
        storage_result(crate::storage::rename_entity_from_query(
            http::params(query, body).as_str(),
        ))
    } else if method == "POST" && path == "/api/entity/select" {
        storage_result(crate::storage::select_entity_from_params(
            http::params(query, body).as_str(),
        ))
    } else if method == "POST" && path == "/api/entity/clear-active" {
        storage_result(crate::storage::clear_active_entity_from_params(
            http::params(query, body).as_str(),
        ))
    } else if method == "POST" && path == "/api/storage/format" {
        storage_result(crate::storage::format_storage())
    } else if method == "POST" && path == "/api/storage/compact" {
        storage_result(crate::storage::compact_storage())
    } else if let Some(id) = path
        .strip_prefix("/api/identity/")
        .and_then(|tail| tail.strip_suffix(".json"))
        .and_then(|raw| raw.parse::<u32>().ok())
    {
        storage_result(crate::storage::identity_json(
            crate::storage::records::RecordId(id),
        ))
    } else if let Some(id) = path
        .strip_prefix("/api/entity/")
        .and_then(|tail| tail.strip_suffix(".bin"))
        .and_then(|raw| raw.parse::<u32>().ok())
    {
        match crate::storage::read_entity_blob(crate::storage::records::RecordId(id)) {
            Ok(body) => Response::Binary {
                status: "200 OK",
                content_type: "application/octet-stream",
                body,
            },
            Err(error) => Response::StaticText {
                status: error.status_code(),
                content_type: "text/plain",
                body: error.message(),
            },
        }
    } else {
        Response::StaticText {
            status: "404 Not Found",
            content_type: "text/plain",
            body: "not found\n",
        }
    }
}

fn storage_result(result: Result<String, crate::storage::StorageError>) -> Response {
    match result {
        Ok(body) => Response::Text {
            status: "200 OK",
            content_type: "application/json",
            body,
        },
        Err(error) => Response::StaticText {
            status: error.status_code(),
            content_type: "text/plain",
            body: error.message(),
        },
    }
}

fn catalog_response(query: &str) -> Response {
    const DEFAULT_LIMIT: usize = 30;
    const MAX_LIMIT: usize = 40;

    let game = query_param(query, "game").unwrap_or_else(|| String::from("skylanders"));
    let kind = query_param(query, "kind");
    let search = query_param(query, "q").unwrap_or_default();
    let offset = query_param(query, "offset")
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let limit = query_param(query, "limit")
        .and_then(|value| value.parse().ok())
        .unwrap_or(DEFAULT_LIMIT)
        .min(MAX_LIMIT);

    if game == "infinity" {
        return Response::Text {
            status: "200 OK",
            content_type: "application/json",
            body: infinity_catalog_json(kind.as_deref(), search.as_str(), offset, limit),
        };
    }
    if game != "skylanders" {
        return Response::StaticText {
            status: "400 Bad Request",
            content_type: "text/plain",
            body: "bad request\n",
        };
    }

    Response::Text {
        status: "200 OK",
        content_type: "application/json",
        body: skylanders_catalog_json(kind.as_deref(), search.as_str(), offset, limit),
    }
}

fn skylanders_catalog_json(
    kind: Option<&str>,
    search: &str,
    offset: usize,
    limit: usize,
) -> String {
    let mut total = 0usize;
    for entry in crate::figures::skylanders::catalog::SKYLANDERS_CATALOG {
        if skylanders_catalog_entry_matches(entry, kind, search) {
            total += 1;
        }
    }

    let mut body = format!(
        "{{\"game\":\"skylanders\",\"offset\":{},\"limit\":{},\"total\":{},\"figures\":[",
        offset, limit, total
    );
    push_skylanders_catalog_entries(&mut body, kind, search, offset, limit);
    body.push_str("],\"skylanders\":[");
    push_skylanders_catalog_entries(&mut body, kind, search, offset, limit);
    body.push_str("]}\n");
    body
}

fn push_skylanders_catalog_entries(
    body: &mut String,
    kind: Option<&str>,
    search: &str,
    offset: usize,
    limit: usize,
) {
    let mut emitted = 0usize;
    let mut seen = 0usize;
    for entry in crate::figures::skylanders::catalog::SKYLANDERS_CATALOG {
        if !skylanders_catalog_entry_matches(entry, kind, search) {
            continue;
        }
        if seen < offset {
            seen += 1;
            continue;
        }
        if emitted >= limit {
            break;
        }
        if emitted > 0 {
            body.push(',');
        }
        emitted += 1;
        push_skylanders_catalog_entry(body, entry);
    }
}

fn infinity_catalog_json(kind: Option<&str>, search: &str, offset: usize, limit: usize) -> String {
    let mut total = 0usize;
    for entry in crate::figures::infinity::INFINITY_CATALOG {
        if infinity_catalog_entry_matches(entry, kind, search) {
            total += 1;
        }
    }

    let mut body = format!(
        "{{\"game\":\"infinity\",\"offset\":{},\"limit\":{},\"total\":{},\"figures\":[",
        offset, limit, total
    );
    let mut emitted = 0usize;
    let mut seen = 0usize;
    for entry in crate::figures::infinity::INFINITY_CATALOG {
        if !infinity_catalog_entry_matches(entry, kind, search) {
            continue;
        }
        if seen < offset {
            seen += 1;
            continue;
        }
        if emitted >= limit {
            break;
        }
        if emitted > 0 {
            body.push(',');
        }
        emitted += 1;
        push_infinity_catalog_entry(&mut body, entry);
    }
    body.push_str("]}\n");
    body
}

fn push_skylanders_catalog_entry(
    body: &mut String,
    entry: &crate::figures::skylanders::catalog::FigureCatalogEntry,
) {
    body.push_str(&format!(
        "{{\"index\":{},\"game\":\"{}\",\"kind\":\"{}\",\"series\":\"{}\",\"name\":\"{}\",\"character_id\":{},\"variant_id\":{}}}",
        entry.index,
        entry.game_line.wire_name(),
        entry.kind.wire_name(),
        entry.series,
        entry.name,
        entry.character_id,
        entry.variant_id
    ));
}

fn push_infinity_catalog_entry(
    body: &mut String,
    entry: &crate::figures::infinity::FigureCatalogEntry,
) {
    body.push_str(&format!(
        "{{\"index\":{},\"game\":\"{}\",\"kind\":\"{}\",\"series\":\"{}\",\"name\":\"{}\",\"figure_number\":{}}}",
        entry.index,
        entry.game_line.wire_name(),
        entry.kind.wire_name(),
        entry.series,
        entry.name,
        entry.figure_number
    ));
}

fn skylanders_catalog_entry_matches(
    entry: &crate::figures::skylanders::catalog::FigureCatalogEntry,
    kind: Option<&str>,
    search: &str,
) -> bool {
    if let Some(kind) = kind {
        if !kind.is_empty() && kind != entry.kind.wire_name() {
            return false;
        }
    }
    if search.is_empty() {
        return true;
    }

    contains_ascii_case_insensitive(entry.name, search)
        || contains_ascii_case_insensitive(entry.series, search)
        || format!("{}", entry.character_id).contains(search)
}

fn infinity_catalog_entry_matches(
    entry: &crate::figures::infinity::FigureCatalogEntry,
    kind: Option<&str>,
    search: &str,
) -> bool {
    if let Some(kind) = kind {
        if !kind.is_empty() && kind != entry.kind.wire_name() {
            return false;
        }
    }
    if search.is_empty() {
        return true;
    }

    contains_ascii_case_insensitive(entry.name, search)
        || contains_ascii_case_insensitive(entry.series, search)
        || format!("{}", entry.figure_number).contains(search)
}

fn contains_ascii_case_insensitive(haystack: &str, needle: &str) -> bool {
    let haystack = haystack.as_bytes();
    let needle = needle.as_bytes();
    if needle.is_empty() {
        return true;
    }
    if needle.len() > haystack.len() {
        return false;
    }

    haystack.windows(needle.len()).any(|window| {
        window
            .iter()
            .zip(needle.iter())
            .all(|(left, right)| left.eq_ignore_ascii_case(right))
    })
}

fn query_param(query: &str, name: &str) -> Option<String> {
    for pair in query.split('&') {
        if let Some((key, value)) = pair.split_once('=') {
            if key == name {
                return Some(percent_decode(value));
            }
        }
    }
    None
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = String::new();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                out.push(' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                if let (Some(high), Some(low)) =
                    (hex_nibble(bytes[index + 1]), hex_nibble(bytes[index + 2]))
                {
                    out.push((high << 4 | low) as char);
                    index += 3;
                } else {
                    index += 1;
                }
            }
            byte => {
                out.push(byte as char);
                index += 1;
            }
        }
    }
    out
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(target_arch = "xtensa")]
fn request_mode_reenumeration() {
    crate::usb::request_reboot_after_usb_flush();
}

#[cfg(not(target_arch = "xtensa"))]
fn request_mode_reenumeration() {}
