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
        let mut rx_buffer = [0; 1024];
        let mut tx_buffer = [0; 1024];
        let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);

        if socket.accept(crate::config::HTTP_PORT).await.is_err() {
            Timer::after(Duration::from_millis(100)).await;
            continue;
        }

        let mut request = [0; 1024];
        match socket.read(&mut request).await {
            Ok(read) => handle_request(&mut socket, &request[..read]).await,
            Err(_) => write_all(&mut socket, routes::BAD_REQUEST_RESPONSE).await,
        }
        let _ = socket.flush().await;
    }
}

async fn handle_request(socket: &mut TcpSocket<'_>, request: &[u8]) {
    let Some(target) = routes::request_target(request) else {
        write_all(socket, routes::BAD_REQUEST_RESPONSE).await;
        return;
    };

    if target == "/" {
        write_all(socket, ui_html::INDEX_RESPONSE).await;
    } else if target == routes::STATUS_PATH {
        let body = format!(
            "{{\"mode\":\"skylanders\",\"active_instance\":null,\"storage\":{}}}\n",
            crate::storage::status_json()
        );
        write_text(socket, "200 OK", "application/json", &body).await;
    } else if target == "/api/library" {
        write_text(
            socket,
            "200 OK",
            "application/json",
            &crate::storage::library_json(),
        )
        .await;
    } else if let Some(query) = target.strip_prefix("/api/identity/create?") {
        write_storage_result(socket, crate::storage::create_identity_from_query(query)).await;
    } else if let Some(query) = target.strip_prefix("/api/instance/create?") {
        write_storage_result(socket, crate::storage::create_instance_from_query(query)).await;
    } else if let Some(query) = target.strip_prefix("/api/identity/delete?") {
        write_storage_result(socket, crate::storage::delete_identity_from_query(query)).await;
    } else if let Some(query) = target.strip_prefix("/api/instance/delete?") {
        write_storage_result(socket, crate::storage::delete_instance_from_query(query)).await;
    } else if let Some(query) = target.strip_prefix("/api/instance/rename?") {
        write_storage_result(socket, crate::storage::rename_instance_from_query(query)).await;
    } else if let Some(id) = target
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
    } else if target == "/api/storage/format" {
        write_storage_result(socket, crate::storage::format_storage()).await;
    } else {
        write_text(socket, "404 Not Found", "text/plain", "not found\n").await;
    }
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
