#[cfg(target_arch = "xtensa")]
mod device;

pub mod infinity;
pub mod skylanders;

#[cfg(feature = "usb-infinity")]
pub const DEVICE_MODE: crate::domain::GameLine = crate::domain::GameLine::Infinity;
#[cfg(not(feature = "usb-infinity"))]
pub const DEVICE_MODE: crate::domain::GameLine = crate::domain::GameLine::Skylanders;

pub fn init() {
    let _ = (skylanders::VID, skylanders::PID);
    let _ = (infinity::VID, infinity::PID);
    let _ = DEVICE_MODE;
}

#[cfg(target_arch = "xtensa")]
pub use device::run;
