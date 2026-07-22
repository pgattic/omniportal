pub const AP_SSID: &str = "OmniPortal";
pub const AP_IP_OCTETS: [u8; 4] = [192, 168, 4, 1];
pub const AP_NETMASK_PREFIX: u8 = 24;
pub const HTTP_PORT: u16 = 80;
pub const DHCP_POOL_START: u8 = 100;
pub const DHCP_POOL_END: u8 = 199;
pub const DHCP_LEASE_SECONDS: u32 = 24 * 60 * 60;

pub const STORAGE_FLASH_OFFSET: u32 = 4 * 1024 * 1024 - STORAGE_FLASH_BYTES;
pub const STORAGE_FLASH_BYTES: u32 = 128 * 1024;
