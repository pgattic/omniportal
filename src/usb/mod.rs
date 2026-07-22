#[cfg(target_arch = "xtensa")]
mod device;
#[cfg(target_arch = "xtensa")]
mod esp32s3;
#[cfg(target_arch = "xtensa")]
mod infinity_device;
#[cfg(target_arch = "xtensa")]
mod skylanders_device;

pub mod infinity;
pub mod skylanders;

#[cfg(target_arch = "xtensa")]
use core::sync::atomic::{AtomicBool, Ordering};

#[cfg(target_arch = "xtensa")]
static REBOOT_AFTER_FLUSH_REQUESTED: AtomicBool = AtomicBool::new(false);

#[cfg(target_arch = "xtensa")]
pub fn request_reboot_after_usb_flush() {
    REBOOT_AFTER_FLUSH_REQUESTED.store(true, Ordering::SeqCst);
}

#[cfg(target_arch = "xtensa")]
pub(super) fn reboot_after_usb_flush_requested() -> bool {
    REBOOT_AFTER_FLUSH_REQUESTED.load(Ordering::SeqCst)
}

#[cfg(target_arch = "xtensa")]
pub use device::run;
