#[macro_export]
macro_rules! rp2350_pico2w_println {
    ($($arg:tt)*) => {
        let _ = core::format_args!($($arg)*);
    };
}

pub use crate::rp2350_pico2w_println as println;
