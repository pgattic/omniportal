#![no_std]
#![no_main]

extern crate alloc;

use esp_backtrace as _;

#[esp_hal::main]
fn main() -> ! {
    omniportal::platform::esp32s3_n16r8::run()
}
