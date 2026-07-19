#[cfg(target_arch = "xtensa")]
use crate::platform::println;
#[cfg(target_arch = "xtensa")]
use alloc::format;
use alloc::string::String;
#[cfg(target_arch = "xtensa")]
use embassy_net::{tcp::TcpSocket, Stack};
#[cfg(target_arch = "xtensa")]
use embassy_time::{Duration, Timer};

use crate::web::{http, routes, ui_html};

#[cfg(target_arch = "xtensa")]
#[embassy_executor::task(pool_size = 2)]
pub async fn run(stack: Stack<'static>) {
    stack.wait_config_up().await;
    println!("HTTP server ready");

    loop {
        let mut rx_buffer = [0; 2048];
        let mut tx_buffer = [0; 2048];
        let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);

        if socket.accept(crate::config::HTTP_PORT).await.is_err() {
            Timer::after(Duration::from_millis(100)).await;
            continue;
        }

        let mut request = [0; 4096];
        match read_request(&mut socket, &mut request).await {
            Ok(read) => handle_request(&mut socket, &request[..read]).await,
            Err(_) => write_all(&mut socket, routes::BAD_REQUEST_RESPONSE).await,
        }
        let _ = socket.flush().await;
    }
}

#[cfg(target_arch = "xtensa")]
async fn handle_request(socket: &mut TcpSocket<'_>, request: &[u8]) {
    let method = routes::request_method(request).unwrap_or("");
    let Some(target) = routes::request_target(request) else {
        write_all(socket, routes::BAD_REQUEST_RESPONSE).await;
        return;
    };
    let (path, query) = http::split_target(target);
    let body = http::request_body(request).unwrap_or(&[]);

    if method == "GET" && path == "/" {
        write_text(socket, "200 OK", "text/html", ui_html::INDEX_HTML).await;
    } else if method == "GET" && path == "/favicon.ico" {
        write_all(
            socket,
            b"HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n",
        )
        .await;
    } else if method == "GET" && path == routes::STATUS_PATH {
        let body = format!(
            "{{\"mode\":\"{}\",\"active_entity\":{},\"active_slots\":{},\"storage\":{}}}\n",
            crate::storage::usb_mode().wire_name(),
            crate::storage::active_entity_json(),
            crate::storage::active_slots_json(),
            crate::storage::status_json()
        );
        write_text(socket, "200 OK", "application/json", &body).await;
    } else if method == "GET" && path == "/api/library" {
        write_text(
            socket,
            "200 OK",
            "application/json",
            &crate::storage::library_json(),
        )
        .await;
    } else if method == "GET" && path == "/api/catalog" {
        write_catalog(socket, query).await;
    } else if method == "POST" && path == "/api/mode/set" {
        let before_mode = crate::storage::usb_mode();
        let result = crate::storage::set_usb_mode_from_params(http::params(query, body).as_str());
        if result.is_ok() && crate::storage::usb_mode() != before_mode {
            crate::usb::request_reboot_after_usb_flush();
        }
        write_storage_result(socket, result).await;
    } else if method == "POST" && path == "/api/identity/create" {
        write_storage_result(
            socket,
            crate::storage::create_identity_from_params(http::params(query, body).as_str()),
        )
        .await;
    } else if method == "POST" && path == "/api/entity/create" {
        write_storage_result(
            socket,
            crate::storage::create_entity_from_params(http::params(query, body).as_str()),
        )
        .await;
    } else if method == "POST" && path == "/api/entity/create-from-catalog" {
        write_storage_result(
            socket,
            crate::storage::create_entity_from_catalog_params(http::params(query, body).as_str()),
        )
        .await;
    } else if method == "POST" && path == "/api/entity/upload" {
        let result = if query.is_empty() {
            crate::storage::upload_entity_from_form_params(http::params(query, body).as_str())
        } else {
            crate::storage::upload_entity_from_params(query, body)
        };
        write_storage_result(socket, result).await;
    } else if method == "POST" && path == "/api/entity/clone" {
        write_storage_result(
            socket,
            crate::storage::clone_entity_from_params(http::params(query, body).as_str()),
        )
        .await;
    } else if method == "POST" && path == "/api/identity/delete" {
        write_storage_result(
            socket,
            crate::storage::delete_identity_from_query(http::params(query, body).as_str()),
        )
        .await;
    } else if method == "POST" && path == "/api/identity/rename" {
        write_storage_result(
            socket,
            crate::storage::rename_identity_from_query(http::params(query, body).as_str()),
        )
        .await;
    } else if method == "POST" && path == "/api/entity/delete" {
        write_storage_result(
            socket,
            crate::storage::delete_entity_from_query(http::params(query, body).as_str()),
        )
        .await;
    } else if method == "POST" && path == "/api/entity/rename" {
        write_storage_result(
            socket,
            crate::storage::rename_entity_from_query(http::params(query, body).as_str()),
        )
        .await;
    } else if method == "POST" && path == "/api/entity/select" {
        write_storage_result(
            socket,
            crate::storage::select_entity_from_params(http::params(query, body).as_str()),
        )
        .await;
    } else if method == "POST" && path == "/api/entity/clear-active" {
        write_storage_result(
            socket,
            crate::storage::clear_active_entity_from_params(http::params(query, body).as_str()),
        )
        .await;
    } else if method == "POST" && path == "/api/storage/format" {
        write_storage_result(socket, crate::storage::format_storage()).await;
    } else if method == "POST" && path == "/api/storage/compact" {
        write_storage_result(socket, crate::storage::compact_storage()).await;
    } else if let Some(id) = path
        .strip_prefix("/api/identity/")
        .and_then(|tail| tail.strip_suffix(".json"))
        .and_then(|raw| raw.parse::<u32>().ok())
    {
        write_storage_result(
            socket,
            crate::storage::identity_json(crate::storage::records::RecordId(id)),
        )
        .await;
    } else if let Some(id) = path
        .strip_prefix("/api/entity/")
        .and_then(|tail| tail.strip_suffix(".bin"))
        .and_then(|raw| raw.parse::<u32>().ok())
    {
        match crate::storage::read_entity_blob(crate::storage::records::RecordId(id)) {
            Ok(data) => write_binary(socket, "200 OK", "application/octet-stream", &data).await,
            Err(error) => {
                write_text(socket, error.status_code(), "text/plain", error.message()).await
            }
        }
    } else {
        write_text(socket, "404 Not Found", "text/plain", "not found\n").await;
    }
}

#[cfg(target_arch = "xtensa")]
async fn write_catalog(socket: &mut TcpSocket<'_>, query: &str) {
    const DEFAULT_LIMIT: usize = 30;
    const MAX_LIMIT: usize = 40;

    let game = query_param(query, "game").unwrap_or_else(|| String::from("skylanders"));
    let kind = query_param(query, "kind");
    let search = query_param(query, "q").unwrap_or_default();
    let offset = query_param(query, "offset")
        .and_then(|value| parse_usize(value.as_str()))
        .unwrap_or(0);
    let limit = query_param(query, "limit")
        .and_then(|value| parse_usize(value.as_str()))
        .unwrap_or(DEFAULT_LIMIT)
        .min(MAX_LIMIT);

    if game == "infinity" {
        let mut total = 0usize;
        for entry in crate::figures::infinity::INFINITY_CATALOG {
            if infinity_catalog_entry_matches(entry, kind.as_deref(), search.as_str()) {
                total += 1;
            }
        }

        let body = format!(
            "{{\"game\":\"infinity\",\"offset\":{},\"limit\":{},\"total\":{},\"figures\":[",
            offset, limit, total
        );
        let mut body = body;
        let mut emitted = 0usize;
        let mut seen = 0usize;
        for entry in crate::figures::infinity::INFINITY_CATALOG {
            if !infinity_catalog_entry_matches(entry, kind.as_deref(), search.as_str()) {
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
        write_text(socket, "200 OK", "application/json", body.as_str()).await;
        return;
    }
    if game != "skylanders" {
        write_text(socket, "400 Bad Request", "text/plain", "bad request\n").await;
        return;
    }

    let mut total = 0usize;

    for entry in crate::figures::skylanders::catalog::SKYLANDERS_CATALOG {
        if catalog_entry_matches(entry, kind.as_deref(), search.as_str()) {
            total += 1;
        }
    }

    let mut body = format!(
        "{{\"game\":\"skylanders\",\"offset\":{},\"limit\":{},\"total\":{},\"figures\":[",
        offset, limit, total
    );
    let mut emitted = 0usize;
    let mut seen = 0usize;
    for entry in crate::figures::skylanders::catalog::SKYLANDERS_CATALOG {
        if !catalog_entry_matches(entry, kind.as_deref(), search.as_str()) {
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
        push_skylanders_catalog_entry(&mut body, entry);
    }

    body.push_str("],\"skylanders\":[");
    emitted = 0;
    seen = 0;
    for entry in crate::figures::skylanders::catalog::SKYLANDERS_CATALOG {
        if !catalog_entry_matches(entry, kind.as_deref(), search.as_str()) {
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
        push_skylanders_catalog_entry(&mut body, entry);
    }
    body.push_str("]}\n");
    write_text(socket, "200 OK", "application/json", body.as_str()).await;
}

#[cfg(target_arch = "xtensa")]
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

#[cfg(target_arch = "xtensa")]
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

#[cfg(target_arch = "xtensa")]
fn catalog_entry_matches(
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

#[cfg(target_arch = "xtensa")]
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

#[cfg(target_arch = "xtensa")]
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

#[cfg(target_arch = "xtensa")]
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

#[cfg(target_arch = "xtensa")]
fn parse_usize(value: &str) -> Option<usize> {
    value.parse().ok()
}

#[cfg(target_arch = "xtensa")]
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

#[cfg(target_arch = "xtensa")]
fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(target_arch = "xtensa")]
async fn read_request(socket: &mut TcpSocket<'_>, buffer: &mut [u8]) -> Result<usize, ()> {
    let mut read = socket.read(buffer).await.map_err(|_| ())?;
    let mut sent_continue = false;
    loop {
        if let Some(body_start) = http::body_start(&buffer[..read]) {
            let content_len = http::content_length(&buffer[..body_start]).unwrap_or(0);
            let needed = body_start + content_len;
            if read >= needed {
                return Ok(needed);
            }
            if needed > buffer.len() {
                return Err(());
            }
            if !sent_continue && http::expects_continue(&buffer[..body_start]) {
                write_all(socket, b"HTTP/1.1 100 Continue\r\n\r\n").await;
                sent_continue = true;
            }
            let chunk = socket
                .read(&mut buffer[read..needed])
                .await
                .map_err(|_| ())?;
            if chunk == 0 {
                return Err(());
            }
            read += chunk;
        } else {
            if read == buffer.len() {
                return Err(());
            }
            let chunk = socket.read(&mut buffer[read..]).await.map_err(|_| ())?;
            if chunk == 0 {
                return Err(());
            }
            read += chunk;
        }
    }
}

#[cfg(target_arch = "xtensa")]
async fn write_storage_result(
    socket: &mut TcpSocket<'_>,
    result: Result<String, crate::storage::StorageError>,
) {
    match result {
        Ok(body) => write_text(socket, "200 OK", "application/json", &body).await,
        Err(error) => {
            println!("HTTP storage result error: {:?}", error);
            write_text(socket, error.status_code(), "text/plain", error.message()).await;
        }
    }
}

#[cfg(target_arch = "xtensa")]
async fn write_text(socket: &mut TcpSocket<'_>, status: &str, content_type: &str, body: &str) {
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    write_all(socket, header.as_bytes()).await;
    write_all(socket, body.as_bytes()).await;
}

#[cfg(target_arch = "xtensa")]
async fn write_binary(socket: &mut TcpSocket<'_>, status: &str, content_type: &str, body: &[u8]) {
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    write_all(socket, header.as_bytes()).await;
    write_all(socket, body).await;
}

#[cfg(target_arch = "xtensa")]
async fn write_all(socket: &mut TcpSocket<'_>, mut bytes: &[u8]) {
    while !bytes.is_empty() {
        match socket.write(bytes).await {
            Ok(0) | Err(_) => break,
            Ok(written) => bytes = &bytes[written..],
        }
    }
}
