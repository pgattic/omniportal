#[cfg(target_arch = "xtensa")]
pub mod esp32s3_n16r8;
#[cfg(target_arch = "arm")]
pub mod rp2350_pico2w;

#[cfg(target_arch = "xtensa")]
pub use esp32s3_n16r8::board;
#[cfg(target_arch = "xtensa")]
pub use esp32s3_n16r8::storage_flash::StorageFlash;
#[cfg(target_arch = "arm")]
pub use rp2350_pico2w::storage_flash::StorageFlash;

#[cfg(target_arch = "xtensa")]
pub use esp32s3_n16r8::log::println;
#[cfg(target_arch = "arm")]
pub use rp2350_pico2w::log::println;
