pub mod board;
pub mod log;
pub mod storage_flash;
pub mod wifi;

use core::mem::MaybeUninit;

use crate::platform::println;
use crate::{dhcp, storage, usb, web};
use embassy_executor::Executor;
use embassy_net::{Ipv4Address, Ipv4Cidr, StackResources, StaticConfigV4};
use esp_hal::rng::Rng;
use esp_hal::{clock::CpuClock, timer::timg::TimerGroup};
use static_cell::StaticCell;

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

pub fn run() -> ! {
    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    init_heap();

    let timer_group = TimerGroup::new(peripherals.TIMG0);
    esp_hal_embassy::init(timer_group.timer0);

    let wifi_timer_group = TimerGroup::new(peripherals.TIMG1);
    static ESP_WIFI_CONTROLLER: StaticCell<esp_wifi::EspWifiController<'static>> =
        StaticCell::new();
    let esp_wifi_controller = ESP_WIFI_CONTROLLER
        .init(esp_wifi::init(wifi_timer_group.timer0, Rng::new(peripherals.RNG)).unwrap());
    let (wifi_controller, wifi_interfaces) =
        esp_wifi::wifi::new(esp_wifi_controller, peripherals.WIFI).unwrap();

    let net_config = embassy_net::Config::ipv4_static(StaticConfigV4 {
        address: Ipv4Cidr::new(
            Ipv4Address::new(
                board::AP_IP_OCTETS[0],
                board::AP_IP_OCTETS[1],
                board::AP_IP_OCTETS[2],
                board::AP_IP_OCTETS[3],
            ),
            board::AP_NETMASK_PREFIX,
        ),
        gateway: None,
        dns_servers: Default::default(),
    });
    static NET_RESOURCES: StaticCell<StackResources<8>> = StaticCell::new();
    let (net_stack, net_runner) = embassy_net::new(
        wifi_interfaces.ap,
        net_config,
        NET_RESOURCES.init(StackResources::new()),
        0x4f4d_4e49,
    );

    println!("OmniPortal ESP32-S3 Embassy firmware starting");

    storage::init();

    static EXECUTOR: StaticCell<Executor> = StaticCell::new();
    let executor = EXECUTOR.init(Executor::new());
    executor.run(|spawner| {
        spawner.spawn(storage::run()).ok();
        spawner
            .spawn(usb::run(
                peripherals.USB0,
                peripherals.GPIO20,
                peripherals.GPIO19,
            ))
            .ok();
        spawner.spawn(wifi::run_network(net_runner)).ok();
        spawner.spawn(wifi::run(wifi_controller)).ok();
        spawner.spawn(dhcp::run(net_stack)).ok();
        for _ in 0..web::HTTP_WORKERS {
            spawner.spawn(web::run(net_stack)).ok();
        }
    });
}

fn init_heap() {
    const HEAP_SIZE: usize = 96 * 1024;
    static mut HEAP: MaybeUninit<[u8; HEAP_SIZE]> = MaybeUninit::uninit();

    unsafe {
        #[allow(static_mut_refs)]
        esp_alloc::HEAP.add_region(esp_alloc::HeapRegion::new(
            HEAP.as_mut_ptr() as *mut u8,
            HEAP_SIZE,
            esp_alloc::MemoryCapability::Internal.into(),
        ));
    }
}
