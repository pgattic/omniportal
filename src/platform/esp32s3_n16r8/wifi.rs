use alloc::string::String;

use crate::platform::println;
use embassy_net::Runner;
use embassy_time::{Duration, Timer};
use esp_wifi::wifi::{
    AccessPointConfiguration, AuthMethod, Configuration, WifiController, WifiDevice,
};

use super::board;

#[embassy_executor::task]
pub async fn run(mut controller: WifiController<'static>) {
    let ap_config = AccessPointConfiguration {
        ssid: String::from(board::AP_SSID),
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

    println!("WiFi AP started: {}", board::AP_SSID);
    println!(
        "HTTP server: http://{}.{}.{}.{}/",
        board::AP_IP_OCTETS[0],
        board::AP_IP_OCTETS[1],
        board::AP_IP_OCTETS[2],
        board::AP_IP_OCTETS[3]
    );

    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}

#[embassy_executor::task]
pub async fn run_network(mut runner: Runner<'static, WifiDevice<'static>>) {
    runner.run().await
}
