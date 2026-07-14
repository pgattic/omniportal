use alloc::format;
use alloc::string::String;
use embassy_net::{tcp::TcpSocket, Stack};
use embassy_time::{Duration, Timer};
use esp_println::println;

pub mod routes;
pub mod ui_html;

pub fn init() {
    let _ = routes::STATUS_PATH;
    let _ = (routes::Route::Index, routes::Route::Status);
    let _ = routes::BAD_REQUEST_RESPONSE;
    let _ = ui_html::INDEX_HTML;
}

#[embassy_executor::task]
pub async fn run(stack: Stack<'static>) {
    stack.wait_config_up().await;
    println!("HTTP server ready");

    loop {
        let mut rx_buffer = [0; 4096];
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

async fn handle_request(socket: &mut TcpSocket<'_>, request: &[u8]) {
    let method = routes::request_method(request).unwrap_or("");
    let Some(target) = routes::request_target(request) else {
        write_all(socket, routes::BAD_REQUEST_RESPONSE).await;
        return;
    };
    let (path, query) = split_target(target);
    let body = request_body(request).unwrap_or(&[]);

    if method == "GET" && path == "/" {
        write_all(socket, ui_html::INDEX_RESPONSE).await;
    } else if method == "GET" && path == routes::STATUS_PATH {
        let body = format!(
            "{{\"mode\":\"skylanders\",\"active_instance\":{},\"storage\":{}}}\n",
            crate::storage::active_instance_json(),
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
    } else if method == "POST" && path == "/api/identity/create" {
        write_storage_result(
            socket,
            crate::storage::create_identity_from_params(params(query, body).as_str()),
        )
        .await;
    } else if method == "POST" && path == "/api/instance/create" {
        write_storage_result(
            socket,
            crate::storage::create_instance_from_params(params(query, body).as_str()),
        )
        .await;
    } else if method == "POST" && path == "/api/instance/upload" {
        write_storage_result(
            socket,
            crate::storage::upload_instance_from_params(query, body),
        )
        .await;
    } else if method == "POST" && path == "/api/backup/upload" {
        write_storage_result(
            socket,
            crate::storage::upload_backup_from_params(query, body),
        )
        .await;
    } else if method == "POST" && path == "/api/instance/clone" {
        write_storage_result(
            socket,
            crate::storage::clone_instance_from_params(params(query, body).as_str()),
        )
        .await;
    } else if method == "POST" && path == "/api/identity/delete" {
        write_storage_result(
            socket,
            crate::storage::delete_identity_from_query(params(query, body).as_str()),
        )
        .await;
    } else if method == "POST" && path == "/api/identity/rename" {
        write_storage_result(
            socket,
            crate::storage::rename_identity_from_query(params(query, body).as_str()),
        )
        .await;
    } else if method == "POST" && path == "/api/instance/delete" {
        write_storage_result(
            socket,
            crate::storage::delete_instance_from_query(params(query, body).as_str()),
        )
        .await;
    } else if method == "POST" && path == "/api/backup/delete" {
        write_storage_result(
            socket,
            crate::storage::delete_backup_from_query(params(query, body).as_str()),
        )
        .await;
    } else if method == "POST" && path == "/api/backup/rename" {
        write_storage_result(
            socket,
            crate::storage::rename_backup_from_query(params(query, body).as_str()),
        )
        .await;
    } else if method == "POST" && path == "/api/instance/rename" {
        write_storage_result(
            socket,
            crate::storage::rename_instance_from_query(params(query, body).as_str()),
        )
        .await;
    } else if method == "POST" && path == "/api/instance/select" {
        write_storage_result(
            socket,
            crate::storage::select_instance_from_params(params(query, body).as_str()),
        )
        .await;
    } else if method == "POST" && path == "/api/instance/clear-active" {
        write_storage_result(socket, crate::storage::clear_active_instance()).await;
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
        .strip_prefix("/api/instance/")
        .and_then(|tail| tail.strip_suffix(".bin"))
        .and_then(|raw| raw.parse::<u32>().ok())
    {
        match crate::storage::read_instance_blob(crate::storage::records::RecordId(id)) {
            Ok(data) => write_binary(socket, "200 OK", "application/octet-stream", &data).await,
            Err(error) => {
                write_text(socket, error.status_code(), "text/plain", error.message()).await
            }
        }
    } else if let Some(id) = path
        .strip_prefix("/api/backup/")
        .and_then(|tail| tail.strip_suffix(".json"))
        .and_then(|raw| raw.parse::<u32>().ok())
    {
        write_storage_result(
            socket,
            crate::storage::backup_json(crate::storage::records::RecordId(id)),
        )
        .await;
    } else if let Some(id) = path
        .strip_prefix("/api/backup/")
        .and_then(|tail| tail.strip_suffix(".bin"))
        .and_then(|raw| raw.parse::<u32>().ok())
    {
        match crate::storage::read_backup_blob(crate::storage::records::RecordId(id)) {
            Ok(data) => write_binary(socket, "200 OK", "application/octet-stream", &data).await,
            Err(error) => {
                write_text(socket, error.status_code(), "text/plain", error.message()).await
            }
        }
    } else {
        write_text(socket, "404 Not Found", "text/plain", "not found\n").await;
    }
}

async fn read_request(socket: &mut TcpSocket<'_>, buffer: &mut [u8]) -> Result<usize, ()> {
    let mut read = socket.read(buffer).await.map_err(|_| ())?;
    loop {
        if let Some(body_start) = body_start(&buffer[..read]) {
            let content_len = content_length(&buffer[..body_start]).unwrap_or(0);
            let needed = body_start + content_len;
            if read >= needed {
                return Ok(needed);
            }
            if needed > buffer.len() {
                return Err(());
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

fn split_target(target: &str) -> (&str, &str) {
    target.split_once('?').unwrap_or((target, ""))
}

fn params(query: &str, body: &[u8]) -> String {
    if !query.is_empty() {
        String::from(query)
    } else {
        core::str::from_utf8(body)
            .map(String::from)
            .unwrap_or_else(|_| String::new())
    }
}

fn request_body(request: &[u8]) -> Option<&[u8]> {
    body_start(request).map(|start| &request[start..])
}

fn body_start(request: &[u8]) -> Option<usize> {
    request
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)
}

fn content_length(request: &[u8]) -> Option<usize> {
    let header = core::str::from_utf8(request).ok()?;
    let header = header.split("\r\n\r\n").next().unwrap_or(header);
    for line in header.lines() {
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                return value.trim().parse().ok();
            }
        }
    }
    None
}

async fn write_storage_result(
    socket: &mut TcpSocket<'_>,
    result: Result<String, crate::storage::StorageError>,
) {
    match result {
        Ok(body) => write_text(socket, "200 OK", "application/json", &body).await,
        Err(error) => write_text(socket, error.status_code(), "text/plain", error.message()).await,
    }
}

async fn write_text(socket: &mut TcpSocket<'_>, status: &str, content_type: &str, body: &str) {
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    write_all(socket, header.as_bytes()).await;
    write_all(socket, body.as_bytes()).await;
}

async fn write_binary(socket: &mut TcpSocket<'_>, status: &str, content_type: &str, body: &[u8]) {
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    write_all(socket, header.as_bytes()).await;
    write_all(socket, body).await;
}

async fn write_all(socket: &mut TcpSocket<'_>, mut bytes: &[u8]) {
    while !bytes.is_empty() {
        match socket.write(bytes).await {
            Ok(0) | Err(_) => break,
            Ok(written) => bytes = &bytes[written..],
        }
    }
}
