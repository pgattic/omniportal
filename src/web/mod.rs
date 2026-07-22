pub mod dispatch;
pub mod http;
pub mod routes;
#[cfg(target_arch = "xtensa")]
mod server;
pub mod ui_html;

pub const HTTP_WORKERS: usize = 2;

pub fn init() {
    let _ = routes::STATUS_PATH;
    let _ = (routes::Route::Index, routes::Route::Status);
    let _ = routes::BAD_REQUEST_RESPONSE;
    let _ = http::split_target(routes::STATUS_PATH);
    let _ = ui_html::INDEX_HTML;
}

#[cfg(target_arch = "xtensa")]
pub use server::run;
