use embassy_time::{Duration, Timer};

pub mod records;
pub mod wear;

pub fn init() {
    let _ = records::MAX_RECORD_NAME_BYTES;
    let _ = records::RecordId(0);
    let _ = wear::DEFAULT_COMMIT_DEBOUNCE_MS;
}

#[embassy_executor::task]
pub async fn run() {
    loop {
        Timer::after(Duration::from_secs(60)).await;
    }
}
