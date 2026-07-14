#![allow(dead_code)]
// Protocol constants and packet helpers are defined before the USB device stack is wired up.

pub const VID: u16 = 0x1430;
pub const PID: u16 = 0x0150;

pub const BCD_USB: u16 = 0x0200;
pub const BCD_DEVICE: u16 = 0x0100;
pub const DEVICE_MAX_PACKET_SIZE: u8 = 64;
pub const CONFIG_TOTAL_LENGTH: u16 = 0x0029;
pub const CONFIG_MAX_POWER_MA: u16 = 500;
pub const INTERFACE_NUMBER: u8 = 0;
pub const HID_CLASS: u8 = 0x03;
pub const INTERRUPT_IN_ENDPOINT: u8 = 0x81;
pub const INTERRUPT_OUT_ENDPOINT: u8 = 0x02;
pub const INTERRUPT_MAX_PACKET_SIZE: u16 = 64;
pub const INTERRUPT_POLL_INTERVAL_MS: u8 = 1;

pub const HID_SET_REPORT_REQUEST_TYPE: u8 = 0x21;
pub const HID_SET_REPORT_REQUEST: u8 = 0x09;

pub const REPORT_BYTES: usize = 32;
pub const MAX_PACKET_BYTES: usize = 64;
pub const MAX_FIGURES: usize = 16;
pub const FIGURE_BLOCK_BYTES: usize = 16;
pub const FIGURE_BLOCK_COUNT: u8 = 64;
pub const FIGURE_IMAGE_BYTES: usize = FIGURE_BLOCK_BYTES * FIGURE_BLOCK_COUNT as usize;
pub const FIRST_FIGURE_SLOT_ID: u8 = 0x10;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum SlotStatus {
    Removed = 0,
    Ready = 1,
    Removing = 2,
    Added = 3,
}

impl SlotStatus {
    pub const fn is_present(self) -> bool {
        matches!(self, Self::Ready | Self::Added)
    }
}

pub type Report = [u8; REPORT_BYTES];

pub fn figure_slot_id(slot: u8) -> Option<u8> {
    if (slot as usize) < MAX_FIGURES {
        Some(FIRST_FIGURE_SLOT_ID | slot)
    } else {
        None
    }
}

pub fn status_report(
    slots: &[SlotStatus; MAX_FIGURES],
    interrupt_counter: u8,
    active: bool,
) -> Report {
    let mut status_bits = 0u32;
    for slot in slots.iter().rev() {
        status_bits <<= 2;
        status_bits |= *slot as u32;
    }

    let mut report = [0; REPORT_BYTES];
    report[0] = b'S';
    report[1..5].copy_from_slice(&status_bits.to_le_bytes());
    report[5] = interrupt_counter;
    report[6] = u8::from(active);
    report
}

pub fn activate_response(active: bool) -> Report {
    let mut report = [0; REPORT_BYTES];
    report[0] = b'A';
    report[1] = u8::from(active);
    report[2] = 0xff;
    report[3] = 0x77;
    report
}

pub fn ready_response() -> Report {
    let mut report = [0; REPORT_BYTES];
    report[0] = b'R';
    report[1] = 0x02;
    report[2] = 0x1b;
    report
}

pub fn audio_firmware_response(major: u8, minor: u8) -> Report {
    let mut report = [0; REPORT_BYTES];
    report[0] = b'M';
    report[1] = major;
    report[2] = 0x00;
    report[3] = minor;
    report
}

pub fn query_response(slot: u8, block: u8, data: &[u8; FIGURE_BLOCK_BYTES]) -> Report {
    let mut report = [0; REPORT_BYTES];
    report[0] = b'Q';
    report[1] = figure_slot_id(slot).unwrap_or(0x01);
    report[2] = block;
    report[3..3 + FIGURE_BLOCK_BYTES].copy_from_slice(data);
    report
}

pub fn query_error_response(block: u8) -> Report {
    let mut report = [0; REPORT_BYTES];
    report[0] = b'Q';
    report[1] = 0x01;
    report[2] = block;
    report
}

pub fn write_response(slot: u8, block: u8, ok: bool) -> Report {
    let mut report = [0; REPORT_BYTES];
    report[0] = b'W';
    report[1] = if ok {
        figure_slot_id(slot).unwrap_or(0x01)
    } else {
        0x01
    };
    report[2] = block;
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_slots_to_portal_slot_ids() {
        assert_eq!(figure_slot_id(0), Some(0x10));
        assert_eq!(figure_slot_id(15), Some(0x1f));
        assert_eq!(figure_slot_id(16), None);
    }

    #[test]
    fn status_report_packs_two_bits_per_slot() {
        let mut slots = [SlotStatus::Removed; MAX_FIGURES];
        slots[0] = SlotStatus::Added;
        slots[1] = SlotStatus::Ready;
        slots[2] = SlotStatus::Removing;

        let report = status_report(&slots, 0x3e, true);

        assert_eq!(report[0], b'S');
        assert_eq!(&report[1..5], &0x27u32.to_le_bytes());
        assert_eq!(report[5], 0x3e);
        assert_eq!(report[6], 0x01);
    }

    #[test]
    fn builds_known_command_responses() {
        assert_eq!(&activate_response(true)[..4], &[b'A', 0x01, 0xff, 0x77]);
        assert_eq!(&ready_response()[..3], &[b'R', 0x02, 0x1b]);
        assert_eq!(
            &audio_firmware_response(0x01, 0x19)[..4],
            &[b'M', 0x01, 0x00, 0x19]
        );
    }

    #[test]
    fn query_response_contains_slot_block_and_sixteen_bytes() {
        let data = [0xab; FIGURE_BLOCK_BYTES];
        let report = query_response(3, 12, &data);

        assert_eq!(report[0], b'Q');
        assert_eq!(report[1], 0x13);
        assert_eq!(report[2], 12);
        assert_eq!(&report[3..19], &data);
    }
}
