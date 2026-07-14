use alloc::string::String;

use crate::config;
use embassy_net::Runner;
use embassy_time::{Duration, Timer};
use esp_println::println;
use esp_wifi::wifi::{
    AccessPointConfiguration, AuthMethod, Configuration, WifiController, WifiDevice,
};

pub fn init() {
    let _ = (config::AP_SSID, config::AP_IP_OCTETS);
}

#[embassy_executor::task]
pub async fn run(mut controller: WifiController<'static>) {
    let ap_config = AccessPointConfiguration {
        ssid: String::from(config::AP_SSID),
        ssid_hidden: false,
        channel: 1,
        secondary_channel: None,
        protocols: Default::default(),
        auth_method: AuthMethod::None,
        password: String::new(),
        max_connections: 4,
    };

    controller
        .set_configuration(&Configuration::AccessPoint(ap_config))
        .unwrap();
    controller.start_async().await.unwrap();

    println!("WiFi AP started: {}", config::AP_SSID);
    println!(
        "HTTP server: http://{}.{}.{}.{}/",
        config::AP_IP_OCTETS[0],
        config::AP_IP_OCTETS[1],
        config::AP_IP_OCTETS[2],
        config::AP_IP_OCTETS[3]
    );

    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}

#[embassy_executor::task]
pub async fn run_network(mut runner: Runner<'static, WifiDevice<'static>>) {
    runner.run().await
}
