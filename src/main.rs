#![no_std]
#![no_main]

use embassy_executor::Executor;
use embassy_time::{Duration, Timer};
use esp_backtrace as _;
use esp_hal::gpio::{Output, OutputConfig};
use esp_hal::{clock::CpuClock, gpio::Level, timer::timg::TimerGroup};
use esp_println::println;
use static_cell::StaticCell;

#[cfg(all(feature = "led-gpio-2", feature = "led-gpio-48"))]
compile_error!("Select only one LED GPIO feature");

#[cfg(feature = "led-gpio-48")]
const LED_GPIO: u8 = 48;

#[cfg(all(feature = "led-gpio-2", not(feature = "led-gpio-48")))]
const LED_GPIO: u8 = 2;

#[unsafe(export_name = "esp_app_desc")]
#[unsafe(link_section = ".rodata_desc")]
#[used]
static ESP_APP_DESC: esp_bootloader_esp_idf::EspAppDesc =
    esp_bootloader_esp_idf::EspAppDesc::new_internal(
        env!("CARGO_PKG_VERSION"),
        env!("CARGO_PKG_NAME"),
        esp_bootloader_esp_idf::BUILD_TIME,
        esp_bootloader_esp_idf::BUILD_DATE,
        esp_bootloader_esp_idf::ESP_IDF_COMPATIBLE_VERSION,
        0,
        199,
        esp_bootloader_esp_idf::MMU_PAGE_SIZE,
    );

#[esp_hal::main]
fn main() -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    let timer_group = TimerGroup::new(peripherals.TIMG0);
    esp_hal_embassy::init(timer_group.timer0);

    println!("OmniPortal ESP32-S3 Embassy blinky starting");

    println!("Blinking GPIO{LED_GPIO}");

    static EXECUTOR: StaticCell<Executor> = StaticCell::new();
    let executor = EXECUTOR.init(Executor::new());
    executor.run(|spawner| {
        #[cfg(feature = "led-gpio-48")]
        let led = Output::new(peripherals.GPIO48, Level::Low, OutputConfig::default());

        #[cfg(all(feature = "led-gpio-2", not(feature = "led-gpio-48")))]
        let led = Output::new(peripherals.GPIO2, Level::Low, OutputConfig::default());

        spawner.spawn(blink(led)).ok();
    });
}

#[embassy_executor::task]
async fn blink(mut led: Output<'static>) {
    let mut high = false;

    loop {
        high = !high;

        if high {
            led.set_high();
        } else {
            led.set_low();
        }

        Timer::after(Duration::from_secs(5)).await;
    }
}
