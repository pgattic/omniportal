#[cfg(target_arch = "xtensa")]
pub use crate::platform::board::*;

#[cfg(not(target_arch = "xtensa"))]
pub const AP_SSID: &str = "OmniPortal";
#[cfg(not(target_arch = "xtensa"))]
pub const AP_IP_OCTETS: [u8; 4] = [192, 168, 4, 1];
#[cfg(not(target_arch = "xtensa"))]
pub const AP_NETMASK_PREFIX: u8 = 24;
#[cfg(not(target_arch = "xtensa"))]
pub const HTTP_PORT: u16 = 80;
#[cfg(not(target_arch = "xtensa"))]
pub const DHCP_POOL_START: u8 = 100;
#[cfg(not(target_arch = "xtensa"))]
pub const DHCP_POOL_END: u8 = 199;
#[cfg(not(target_arch = "xtensa"))]
pub const DHCP_LEASE_SECONDS: u32 = 24 * 60 * 60;

#[cfg(not(target_arch = "xtensa"))]
pub const STORAGE_FLASH_OFFSET: u32 = 0;
#[cfg(not(target_arch = "xtensa"))]
pub const STORAGE_FLASH_BYTES: u32 = 256 * 1024;
