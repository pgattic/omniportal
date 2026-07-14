use embassy_net::{tcp::TcpSocket, Stack};
use embassy_time::{Duration, Timer};
use esp_println::println;

pub mod routes;
pub mod ui_html;

pub fn init() {
    let _ = routes::STATUS_PATH;
    let _ = (routes::Route::Index, routes::Route::Status);
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

        let mut request = [0; 512];
        let response = match socket.read(&mut request).await {
            Ok(read) if routes::is_status_request(&request[..read]) => routes::STATUS_RESPONSE,
            Ok(_) => ui_html::INDEX_RESPONSE,
            Err(_) => routes::BAD_REQUEST_RESPONSE,
        };

        write_all(&mut socket, response).await;
        let _ = socket.flush().await;
    }
}

async fn write_all(socket: &mut TcpSocket<'_>, mut bytes: &[u8]) {
    while !bytes.is_empty() {
        match socket.write(bytes).await {
            Ok(0) | Err(_) => break,
            Ok(written) => bytes = &bytes[written..],
        }
    }
}
