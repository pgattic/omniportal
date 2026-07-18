use esp_hal::peripherals::{GPIO19, GPIO20, USB0};

use crate::{domain::GameLine, platform::println, storage};

#[embassy_executor::task]
pub async fn run(usb0: USB0<'static>, usb_dp: GPIO20<'static>, usb_dm: GPIO19<'static>) {
    let mode = storage::usb_mode();
    println!("USB device mode selected: {}", mode.wire_name());
    match mode {
        GameLine::Skylanders => super::skylanders_device::run(usb0, usb_dp, usb_dm).await,
        GameLine::Infinity => super::infinity_device::run(usb0, usb_dp, usb_dm).await,
    }
}
