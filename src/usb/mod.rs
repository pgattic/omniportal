#[cfg(target_arch = "xtensa")]
mod device;
#[cfg(target_arch = "xtensa")]
mod infinity_device;
#[cfg(target_arch = "xtensa")]
mod skylanders_device;

pub mod infinity;
pub mod skylanders;

pub fn init() {
    let _ = (skylanders::VID, skylanders::PID);
    let _ = (infinity::VID, infinity::PID);
}

#[cfg(target_arch = "xtensa")]
pub use device::run;
