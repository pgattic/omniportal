#![no_std]
#![no_main]

extern crate alloc;

mod config;
mod dhcp;
mod figures;
mod platform;
mod state;
mod storage;
mod usb;
mod web;

use esp_backtrace as _;

#[esp_hal::main]
fn main() -> ! {
    platform::esp32s3_n16r8::run()
}
