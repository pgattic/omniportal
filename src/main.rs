#![no_std]
#![no_main]

extern crate alloc;

#[cfg(target_arch = "xtensa")]
use esp_backtrace as _;

#[cfg(target_arch = "arm")]
use panic_halt as _;
#[cfg(target_arch = "arm")]
use rp235x_hal as hal;

#[cfg(target_arch = "arm")]
#[global_allocator]
static HEAP: embedded_alloc::LlffHeap = embedded_alloc::LlffHeap::empty();

#[cfg(target_arch = "arm")]
#[unsafe(link_section = ".start_block")]
#[used]
pub static IMAGE_DEF: hal::block::Block<3> = hal::block::Block::new([
    hal::block::item_image_type_exe(hal::block::Security::Secure, hal::block::Architecture::Arm),
    hal::block::item_vector_table(0x1000_1000)[0],
    hal::block::item_vector_table(0x1000_1000)[1],
]);

#[esp_hal::main]
#[cfg(target_arch = "xtensa")]
fn main() -> ! {
    omniportal::platform::esp32s3_n16r8::run()
}

#[hal::entry]
#[cfg(target_arch = "arm")]
fn main() -> ! {
    init_arm_heap();
    omniportal::platform::rp2350_pico2w::run()
}

#[cfg(target_arch = "arm")]
fn init_arm_heap() {
    const HEAP_SIZE: usize = 64 * 1024;
    static mut HEAP_MEM: [u8; HEAP_SIZE] = [0; HEAP_SIZE];

    unsafe {
        #[allow(static_mut_refs)]
        HEAP.init(HEAP_MEM.as_ptr() as usize, HEAP_SIZE);
    }
}
