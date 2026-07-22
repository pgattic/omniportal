use alloc::format;

use crate::platform::println;
use crate::web::{dispatch, http};
use embassy_net::{tcp::TcpSocket, Stack};
use embassy_time::{Duration, Timer};

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
            Ok(read) => {
                write_response(&mut socket, dispatch::handle_request(&request[..read])).await
            }
            Err(_) => write_response(&mut socket, dispatch::Response::bad_request()).await,
        }
        let _ = socket.flush().await;
    }
}

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

async fn write_response(socket: &mut TcpSocket<'_>, response: dispatch::Response) {
    match response {
        dispatch::Response::Text {
            status,
            content_type,
            body,
        } => write_text(socket, status, content_type, body.as_str()).await,
        dispatch::Response::StaticText {
            status,
            content_type,
            body,
        } => write_text(socket, status, content_type, body).await,
        dispatch::Response::Binary {
            status,
            content_type,
            body,
        } => write_binary(socket, status, content_type, body.as_slice()).await,
        dispatch::Response::Raw(bytes) => write_all(socket, bytes).await,
    }
}

async fn write_text(socket: &mut TcpSocket<'_>, status: &str, content_type: &str, body: &str) {
    write_binary(socket, status, content_type, body.as_bytes()).await;
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
