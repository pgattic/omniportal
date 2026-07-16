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
pub const HID_GET_REPORT_REQUEST: u8 = 0x01;
pub const HID_GET_IDLE_REQUEST: u8 = 0x02;
pub const HID_SET_IDLE_REQUEST: u8 = 0x0a;
pub const HID_DESCRIPTOR_TYPE: u8 = 0x21;
pub const HID_REPORT_DESCRIPTOR_TYPE: u8 = 0x22;

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

// Vendor-defined HID payload: 32-byte input, output, and feature reports.
pub const HID_REPORT_DESCRIPTOR: &[u8] = &[
    0x06,
    0x00,
    0xff, // Usage Page (Vendor Defined)
    0x09,
    0x01, // Usage (1)
    0xa1,
    0x01, // Collection (Application)
    0x15,
    0x00, // Logical Minimum (0)
    0x26,
    0xff,
    0x00, // Logical Maximum (255)
    0x75,
    0x08, // Report Size (8)
    0x95,
    REPORT_BYTES as u8, // Report Count
    0x09,
    0x01, // Usage (1)
    0x81,
    0x02, // Input (Data, Variable, Absolute)
    0x95,
    REPORT_BYTES as u8, // Report Count
    0x09,
    0x02, // Usage (2)
    0x91,
    0x02, // Output (Data, Variable, Absolute)
    0x95,
    REPORT_BYTES as u8, // Report Count
    0x09,
    0x03, // Usage (3)
    0xb1,
    0x02, // Feature (Data, Variable, Absolute)
    0xc0, // End Collection
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PortalState {
    pub active: bool,
    pub slots: [SlotStatus; MAX_FIGURES],
    pub interrupt_counter: u8,
}

impl PortalState {
    pub const fn new() -> Self {
        Self {
            active: true,
            slots: [SlotStatus::Removed; MAX_FIGURES],
            interrupt_counter: 0,
        }
    }

    pub fn next_status_report(&mut self) -> Report {
        let report = status_report(&self.slots, self.interrupt_counter, self.active);
        self.interrupt_counter = self.interrupt_counter.wrapping_add(1);
        report
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommandResponse {
    pub report: Report,
    pub queue_report: bool,
}

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

pub fn handle_command(state: &mut PortalState, command: &[u8]) -> Option<CommandResponse> {
    let op = *command.first()?;
    match op {
        b'A' => {
            state.active = command.get(1).copied().unwrap_or(0) != 0;
            Some(CommandResponse {
                report: activate_response(state.active),
                queue_report: true,
            })
        }
        b'R' => Some(CommandResponse {
            report: ready_response(),
            queue_report: true,
        }),
        b'S' => Some(CommandResponse {
            report: state.next_status_report(),
            queue_report: false,
        }),
        b'Q' => {
            let block = command.get(2).copied().unwrap_or(0);
            Some(CommandResponse {
                report: query_error_response(block),
                queue_report: true,
            })
        }
        b'W' => {
            let block = command.get(2).copied().unwrap_or(0);
            Some(CommandResponse {
                report: write_response(0xff, block, false),
                queue_report: true,
            })
        }
        b'M' => Some(CommandResponse {
            report: audio_firmware_response(command.get(1).copied().unwrap_or(0), 0x19),
            queue_report: true,
        }),
        b'J' => Some(CommandResponse {
            report: ack_response(op),
            queue_report: true,
        }),
        b'C' | b'L' | b'V' | b'Z' => Some(CommandResponse {
            report: ack_response(op),
            queue_report: false,
        }),
        _ => None,
    }
}

fn ack_response(op: u8) -> Report {
    let mut report = [0; REPORT_BYTES];
    report[0] = op;
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

    #[test]
    fn command_handler_updates_activation_state() {
        let mut state = PortalState::new();

        let response = handle_command(&mut state, &[b'A', 1]).unwrap();

        assert!(state.active);
        assert!(response.queue_report);
        assert_eq!(&response.report[..4], &[b'A', 0x01, 0xff, 0x77]);
    }

    #[test]
    fn command_handler_returns_status_reports() {
        let mut state = PortalState::new();
        state.slots[0] = SlotStatus::Ready;

        let first = handle_command(&mut state, &[b'S']).unwrap().report;
        let second = handle_command(&mut state, &[b'S']).unwrap().report;

        assert_eq!(first[0], b'S');
        assert_eq!(&first[1..5], &0x01u32.to_le_bytes());
        assert_eq!(first[5], 0);
        assert_eq!(second[5], 1);
    }

    #[test]
    fn command_handler_accepts_convenience_commands() {
        let mut state = PortalState::new();

        assert_eq!(
            &handle_command(&mut state, &[b'M']).unwrap().report[..4],
            &[b'M', 0x00, 0x00, 0x19]
        );
        assert!(!handle_command(&mut state, &[b'C']).unwrap().queue_report);
        assert!(!handle_command(&mut state, &[b'V']).unwrap().queue_report);
        assert!(handle_command(&mut state, &[b'J']).unwrap().queue_report);
    }

    #[test]
    fn command_handler_stubs_figure_io_as_no_figure_error() {
        let mut state = PortalState::new();

        let query = handle_command(&mut state, &[b'Q', 0x10, 0x02]).unwrap();
        let write = handle_command(&mut state, &[b'W', 0x10, 0x03]).unwrap();

        assert_eq!(&query.report[..3], &[b'Q', 0x01, 0x02]);
        assert_eq!(&write.report[..3], &[b'W', 0x01, 0x03]);
    }
}
