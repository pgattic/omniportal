pub mod http;
pub mod routes;
#[cfg(target_arch = "xtensa")]
mod server;

pub const HTTP_WORKERS: usize = 2;

#[cfg(target_arch = "xtensa")]
pub use server::run;
