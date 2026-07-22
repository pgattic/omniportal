use core::{cmp, ptr};

use critical_section::with;
use rp235x_hal::rom_data;

const XIP_BASE: u32 = 0x1000_0000;
const FLASH_BYTES: u32 = 4 * 1024 * 1024;
const SECTOR_BYTES: u32 = 4096;
const PAGE_BYTES: usize = 256;
const BLOCK_ERASE_CMD: u8 = 0xd8;

pub struct StorageFlash;

impl StorageFlash {
    pub fn new() -> Self {
        Self
    }

    pub fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), ()> {
        let end = offset.checked_add(bytes.len() as u32).ok_or(())?;
        if end > FLASH_BYTES {
            return Err(());
        }

        unsafe {
            let source = (XIP_BASE + offset) as *const u8;
            ptr::copy_nonoverlapping(source, bytes.as_mut_ptr(), bytes.len());
        }
        Ok(())
    }

    pub fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), ()> {
        if bytes.is_empty() {
            return Ok(());
        }
        let end = offset.checked_add(bytes.len() as u32).ok_or(())?;
        if end > FLASH_BYTES {
            return Err(());
        }

        let mut written = 0;
        while written < bytes.len() {
            let absolute = offset + written as u32;
            let page_start = absolute & !((PAGE_BYTES as u32) - 1);
            let page_offset = (absolute - page_start) as usize;
            let count = cmp::min(PAGE_BYTES - page_offset, bytes.len() - written);

            let mut page = [0xff; PAGE_BYTES];
            self.read(page_start, &mut page)?;

            for index in 0..count {
                let old = page[page_offset + index];
                let new = bytes[written + index];
                if old & new != new {
                    return Err(());
                }
                page[page_offset + index] = new;
            }

            with(|_| unsafe {
                rom_data::connect_internal_flash();
                rom_data::flash_exit_xip();
                rom_data::flash_range_program(page_start, page.as_ptr(), PAGE_BYTES);
                rom_data::flash_flush_cache();
                rom_data::flash_enter_cmd_xip();
            });

            written += count;
        }

        Ok(())
    }

    pub fn erase(&mut self, from: u32, to: u32) -> Result<(), ()> {
        if from > to || from % SECTOR_BYTES != 0 || to % SECTOR_BYTES != 0 || to > FLASH_BYTES {
            return Err(());
        }
        if from == to {
            return Ok(());
        }

        with(|_| unsafe {
            rom_data::connect_internal_flash();
            rom_data::flash_exit_xip();
            rom_data::flash_range_erase(from, (to - from) as usize, SECTOR_BYTES, BLOCK_ERASE_CMD);
            rom_data::flash_flush_cache();
            rom_data::flash_enter_cmd_xip();
        });
        Ok(())
    }
}
