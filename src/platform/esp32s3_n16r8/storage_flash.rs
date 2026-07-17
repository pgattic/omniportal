use embedded_storage::nor_flash::{NorFlash, ReadNorFlash};
use esp_storage::FlashStorage;

pub struct StorageFlash {
    inner: FlashStorage,
}

impl StorageFlash {
    pub fn new() -> Self {
        Self {
            inner: FlashStorage::new(),
        }
    }

    pub fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), ()> {
        ReadNorFlash::read(&mut self.inner, offset, bytes).map_err(|_| ())
    }

    pub fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), ()> {
        self.inner.write(offset, bytes).map_err(|_| ())
    }

    pub fn erase(&mut self, from: u32, to: u32) -> Result<(), ()> {
        self.inner.erase(from, to).map_err(|_| ())
    }
}
