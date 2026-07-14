use embassy_time::{Duration, Timer};

pub mod routes;
pub mod ui_html;

pub fn init() {
    let _ = routes::STATUS_PATH;
    let _ = (routes::Route::Index, routes::Route::Status);
    let _ = ui_html::INDEX_HTML;
}

#[embassy_executor::task]
pub async fn run() {
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}
