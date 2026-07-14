use embassy_time::{Duration, Timer};

use crate::config;

pub fn init() {
    let _ = (config::AP_SSID, config::AP_IP_OCTETS);
}

#[embassy_executor::task]
pub async fn run() {
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}
