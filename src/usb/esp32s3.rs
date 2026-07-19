use embedded_hal_02::blocking::delay::DelayMs;
use esp_hal::{
    delay::Delay,
    otg_fs::{Usb, UsbBus as EspUsbBus},
};
use usb_device::device::UsbDevice;

use crate::platform::println;

pub(super) fn force_host_reenumeration(
    usb_dev: &mut UsbDevice<EspUsbBus<Usb<'static>>>,
    label: &str,
) {
    // ESP32-S3 board note:
    // `usb-device::UsbDevice::force_reset()` dispatches through the `UsbBus`
    // trait. With esp-synopsys-usb-otg 0.4.x, that trait method is not
    // overridden and reports `Unsupported`; the working soft disconnect/reconnect
    // is only exposed as this concrete bus method, with an embedded-hal 0.2 delay.
    //
    // Keep this workaround ESP-specific. When adding another board, try the
    // standard `UsbDevice::force_reset()` path first, and only add a board helper
    // if that USB bus has the same trait/inherent-method mismatch.
    let mut delay = UsbResetDelay(Delay::new());
    match usb_dev.bus().force_reset(&mut delay) {
        Ok(()) => println!("{} USB forced host re-enumeration", label),
        Err(error) => println!("{} USB force re-enumeration failed: {:?}", label, error),
    }
}

struct UsbResetDelay(Delay);

impl DelayMs<u32> for UsbResetDelay {
    fn delay_ms(&mut self, ms: u32) {
        self.0.delay_millis(ms);
    }
}
