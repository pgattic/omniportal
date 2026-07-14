use embassy_time::{Duration, Timer};

pub mod infinity;
pub mod skylanders;

pub fn init() {
    let _ = (skylanders::VID, skylanders::PID);
    let _ = (infinity::VID, infinity::PID);
}

#[embassy_executor::task]
pub async fn run() {
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}
