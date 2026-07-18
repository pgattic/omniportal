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
pub const HID_GET_PROTOCOL_REQUEST: u8 = 0x03;
pub const HID_SET_IDLE_REQUEST: u8 = 0x0a;
pub const HID_SET_PROTOCOL_REQUEST: u8 = 0x0b;
pub const HID_DESCRIPTOR_TYPE: u8 = 0x21;
pub const HID_REPORT_DESCRIPTOR_TYPE: u8 = 0x22;

pub const REPORT_BYTES: usize = 32;
pub const MAX_PACKET_BYTES: usize = 64;
pub const MAX_FIGURES: usize = 16;
pub const FIGURE_BLOCK_BYTES: usize = 16;
pub const FIGURE_BLOCK_COUNT: u8 = 64;
pub const FIGURE_IMAGE_BYTES: usize = FIGURE_BLOCK_BYTES * FIGURE_BLOCK_COUNT as usize;
pub const FIRST_FIGURE_SLOT_ID: u8 = 0x10;
// The Wii can miss a one-report placement edge while polling other portal state.
// Hold transient Added/Removing statuses across several reports before settling.
pub const PLACEMENT_STATUS_HOLD_REPORTS: u8 = 8;

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
    status_updated: bool,
    pub interrupt_counter: u8,
    slots: [PortalSlot; MAX_FIGURES],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PortalSlot {
    active_entity_id: Option<u32>,
    slot_status: SlotStatus,
    queued_status: [Option<SlotStatus>; 8],
    queued_status_hold: u8,
    image: [u8; FIGURE_IMAGE_BYTES],
    dirty: bool,
}

impl PortalState {
    pub const fn new() -> Self {
        Self {
            active: true,
            status_updated: false,
            interrupt_counter: 0,
            slots: [PortalSlot::new(); MAX_FIGURES],
        }
    }
}

impl PortalSlot {
    const fn new() -> Self {
        Self {
            active_entity_id: None,
            slot_status: SlotStatus::Removed,
            queued_status: [None; 8],
            queued_status_hold: 0,
            image: [0; FIGURE_IMAGE_BYTES],
            dirty: false,
        }
    }
}

impl PortalState {
    pub fn active_entity_id(&self) -> Option<u32> {
        self.slots.iter().find_map(|slot| slot.active_entity_id)
    }

    pub fn slot_entity_id(&self, slot: u8) -> Option<u32> {
        self.slots
            .get(slot as usize)
            .and_then(|slot| slot.active_entity_id)
    }

    pub fn is_dirty(&self) -> bool {
        self.slots.iter().any(|slot| slot.dirty)
    }

    pub fn is_slot_dirty(&self, slot: u8) -> bool {
        self.slots
            .get(slot as usize)
            .map(|slot| slot.dirty)
            .unwrap_or(false)
    }

    pub fn clear_dirty(&mut self) {
        for slot in &mut self.slots {
            slot.dirty = false;
        }
    }

    pub fn clear_slot_dirty(&mut self, slot: u8) {
        if let Some(slot) = self.slots.get_mut(slot as usize) {
            slot.dirty = false;
        }
    }

    pub fn image(&self) -> &[u8; FIGURE_IMAGE_BYTES] {
        &self.slots[0].image
    }

    pub fn slot_image(&self, slot: u8) -> Option<&[u8; FIGURE_IMAGE_BYTES]> {
        self.slots.get(slot as usize).map(|slot| &slot.image)
    }

    pub fn load_entity(&mut self, entity_id: u32, image: &[u8]) -> bool {
        self.load_entity_into_slot(0, entity_id, image)
    }

    pub fn load_entity_into_slot(&mut self, slot: u8, entity_id: u32, image: &[u8]) -> bool {
        if image.len() != FIGURE_IMAGE_BYTES {
            return false;
        }
        let Some(slot) = self.slots.get_mut(slot as usize) else {
            return false;
        };

        let was_present = slot.active_entity_id.is_some() && slot.slot_status.is_present();
        slot.active_entity_id = Some(entity_id);
        slot.image.copy_from_slice(image);
        slot.slot_status = SlotStatus::Added;
        slot.queued_status_hold = 0;
        slot.queued_status = if was_present {
            status_queue(&[
                SlotStatus::Removing,
                SlotStatus::Removed,
                SlotStatus::Added,
                SlotStatus::Ready,
            ])
        } else {
            status_queue(&[SlotStatus::Added, SlotStatus::Ready])
        };
        slot.dirty = false;
        true
    }

    pub fn clear_entity(&mut self) {
        self.clear_slot(0);
    }

    pub fn clear_slot(&mut self, slot: u8) {
        let Some(slot) = self.slots.get_mut(slot as usize) else {
            return;
        };
        if slot.active_entity_id.is_none() && slot.slot_status == SlotStatus::Removed {
            return;
        }
        slot.active_entity_id = None;
        slot.slot_status = SlotStatus::Removing;
        slot.queued_status_hold = 0;
        slot.queued_status = status_queue(&[SlotStatus::Removing, SlotStatus::Removed]);
        slot.dirty = false;
    }

    pub fn clear_all_entities(&mut self) {
        for slot in 0..MAX_FIGURES {
            self.clear_slot(slot as u8);
        }
    }

    pub fn activate(&mut self) {
        if self.active {
            return;
        }

        for slot in &mut self.slots {
            if slot.slot_status.is_present() {
                slot.queue_present_cycle();
            }
        }
        self.active = true;
    }

    pub fn deactivate(&mut self) {
        for slot in &mut self.slots {
            slot.collapse_for_deactivate();
        }
        self.active = false;
    }

    pub fn update_status(&mut self) {
        if self.status_updated {
            return;
        }

        for slot in &mut self.slots {
            if !slot.slot_status.is_present() {
                continue;
            }
            slot.enqueue_statuses(&[SlotStatus::Removing, SlotStatus::Added, SlotStatus::Ready]);
        }
        self.status_updated = true;
    }

    pub fn next_status_report(&mut self) -> Report {
        let mut slots = [SlotStatus::Removed; MAX_FIGURES];
        for (index, slot) in self.slots.iter_mut().enumerate() {
            slots[index] = slot.next_status();
        }
        let report = status_report(&slots, self.interrupt_counter, self.active);
        self.interrupt_counter = self.interrupt_counter.wrapping_add(1);
        report
    }

    pub fn query_block(&self, slot: u8, block: u8) -> Report {
        let Some(slot_state) = self.slots.get(slot as usize) else {
            return query_error_response(block);
        };
        if block >= FIGURE_BLOCK_COUNT || slot_state.slot_status != SlotStatus::Ready {
            return query_error_response(block);
        }

        let mut data = [0; FIGURE_BLOCK_BYTES];
        let offset = block as usize * FIGURE_BLOCK_BYTES;
        data.copy_from_slice(&slot_state.image[offset..offset + FIGURE_BLOCK_BYTES]);
        query_response(slot, block, &data)
    }

    pub fn write_block(&mut self, slot: u8, block: u8, data: &[u8]) -> Report {
        let Some(slot_state) = self.slots.get_mut(slot as usize) else {
            return write_response(0xff, block, false);
        };
        if block >= FIGURE_BLOCK_COUNT
            || !slot_state.slot_status.is_present()
            || data.len() < FIGURE_BLOCK_BYTES
        {
            return write_response(0xff, block, false);
        }

        let offset = block as usize * FIGURE_BLOCK_BYTES;
        slot_state.image[offset..offset + FIGURE_BLOCK_BYTES]
            .copy_from_slice(&data[..FIGURE_BLOCK_BYTES]);
        slot_state.dirty = true;
        write_response(slot, block, true)
    }
}

impl PortalSlot {
    fn queue_present_cycle(&mut self) {
        self.queued_status_hold = 0;
        self.enqueue_statuses(&[SlotStatus::Added, SlotStatus::Ready]);
    }

    fn enqueue_statuses(&mut self, statuses: &[SlotStatus]) {
        for status in statuses {
            if let Some(slot) = self.queued_status.iter_mut().find(|slot| slot.is_none()) {
                *slot = Some(*status);
            }
        }
    }

    fn collapse_for_deactivate(&mut self) {
        if self.queued_status.iter().any(Option::is_some) {
            for index in (0..self.queued_status.len()).rev() {
                if let Some(status) = self.queued_status[index] {
                    self.slot_status = status;
                    break;
                }
            }
            self.queued_status = [None; 8];
            self.queued_status_hold = 0;
        }

        self.slot_status = if self.slot_status.is_present() {
            SlotStatus::Ready
        } else {
            SlotStatus::Removed
        };
    }

    fn next_status(&mut self) -> SlotStatus {
        if self.queued_status_hold > 0 {
            self.queued_status_hold -= 1;
            return self.slot_status;
        }

        if let Some(status) = pop_status(&mut self.queued_status) {
            self.slot_status = status;
            self.queued_status_hold = PLACEMENT_STATUS_HOLD_REPORTS.saturating_sub(1);
        }
        self.slot_status
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

pub fn color_response(red: u8, green: u8, blue: u8) -> Report {
    let mut report = [0; REPORT_BYTES];
    report[0] = b'C';
    report[1] = red;
    report[2] = green;
    report[3] = blue;
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
            if command.get(1).copied().unwrap_or(0) != 0 {
                state.activate();
            } else {
                state.deactivate();
            }
            Some(CommandResponse {
                report: activate_response(state.active),
                queue_report: true,
            })
        }
        b'R' => {
            state.update_status();
            Some(CommandResponse {
                report: ready_response(),
                queue_report: true,
            })
        }
        b'S' => Some(CommandResponse {
            report: state.next_status_report(),
            queue_report: false,
        }),
        b'Q' => {
            let slot = command_slot(command.get(1).copied().unwrap_or(0xff));
            let block = command.get(2).copied().unwrap_or(0);
            Some(CommandResponse {
                report: slot
                    .map(|slot| state.query_block(slot, block))
                    .unwrap_or_else(|| query_error_response(block)),
                queue_report: true,
            })
        }
        b'W' => {
            let slot = command_slot(command.get(1).copied().unwrap_or(0xff));
            let block = command.get(2).copied().unwrap_or(0);
            Some(CommandResponse {
                report: slot
                    .map(|slot| state.write_block(slot, block, command.get(3..).unwrap_or(&[])))
                    .unwrap_or_else(|| write_response(0xff, block, false)),
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
        b'C' => Some(CommandResponse {
            report: color_response(
                command.get(1).copied().unwrap_or(0),
                command.get(2).copied().unwrap_or(0),
                command.get(3).copied().unwrap_or(0),
            ),
            queue_report: false,
        }),
        b'L' | b'V' | b'Z' => Some(CommandResponse {
            report: ack_response(op),
            queue_report: false,
        }),
        _ => None,
    }
}

fn command_slot(slot_id: u8) -> Option<u8> {
    let slot = slot_id & 0x0f;
    if (slot as usize) < MAX_FIGURES {
        Some(slot)
    } else {
        None
    }
}

fn status_queue(statuses: &[SlotStatus]) -> [Option<SlotStatus>; 8] {
    let mut queue = [None; 8];
    for (index, status) in statuses.iter().take(queue.len()).enumerate() {
        queue[index] = Some(*status);
    }
    queue
}

fn pop_status(queue: &mut [Option<SlotStatus>; 8]) -> Option<SlotStatus> {
    let status = queue[0]?;
    for index in 1..queue.len() {
        queue[index - 1] = queue[index];
    }
    queue[queue.len() - 1] = None;
    Some(status)
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
        assert_eq!(command_slot(0x10), Some(0));
        assert_eq!(command_slot(0x20), Some(0));
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
            &color_response(0x12, 0x34, 0x56)[..4],
            &[b'C', 0x12, 0x34, 0x56]
        );
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
        let image = [0; FIGURE_IMAGE_BYTES];
        assert!(state.load_entity(1, &image));

        let first = handle_command(&mut state, &[b'S']).unwrap().report;

        assert_eq!(first[0], b'S');
        assert_eq!(&first[1..5], &0x03u32.to_le_bytes());
        assert_eq!(first[5], 0);
        for _ in 1..PLACEMENT_STATUS_HOLD_REPORTS {
            assert_eq!(
                handle_command(&mut state, &[b'S']).unwrap().report[1],
                SlotStatus::Added as u8
            );
        }
        let ready = handle_command(&mut state, &[b'S']).unwrap().report;
        assert_eq!(&ready[1..5], &0x01u32.to_le_bytes());
    }

    #[test]
    fn loading_entity_holds_added_status_before_ready() {
        let mut state = PortalState::new();
        assert!(state.load_entity(42, &[0; FIGURE_IMAGE_BYTES]));

        for _ in 0..PLACEMENT_STATUS_HOLD_REPORTS {
            assert_eq!(state.next_status_report()[1], SlotStatus::Added as u8);
        }
        assert_eq!(state.next_status_report()[1], SlotStatus::Ready as u8);
        assert_eq!(state.next_status_report()[1], SlotStatus::Ready as u8);
    }

    #[test]
    fn activate_reannounces_already_present_entities_only_after_deactivate() {
        let mut state = PortalState::new();
        assert!(state.load_entity(42, &[0; FIGURE_IMAGE_BYTES]));
        advance_slot_status(&mut state, SlotStatus::Ready);

        let response = handle_command(&mut state, &[b'A', 1]).unwrap();

        assert_eq!(&response.report[..4], &[b'A', 0x01, 0xff, 0x77]);
        assert_eq!(state.next_status_report()[1], SlotStatus::Ready as u8);

        let response = handle_command(&mut state, &[b'A', 0]).unwrap();
        assert_eq!(&response.report[..4], &[b'A', 0x00, 0xff, 0x77]);

        let response = handle_command(&mut state, &[b'A', 1]).unwrap();
        assert_eq!(&response.report[..4], &[b'A', 0x01, 0xff, 0x77]);
        for _ in 0..PLACEMENT_STATUS_HOLD_REPORTS {
            assert_eq!(state.next_status_report()[1], SlotStatus::Added as u8);
        }
        assert_eq!(state.next_status_report()[1], SlotStatus::Ready as u8);
    }

    #[test]
    fn command_handler_accepts_convenience_commands() {
        let mut state = PortalState::new();

        assert_eq!(
            &handle_command(&mut state, &[b'M']).unwrap().report[..4],
            &[b'M', 0x00, 0x00, 0x19]
        );
        assert_eq!(
            &handle_command(&mut state, &[b'C', 0xff, 0xee, 0xdd])
                .unwrap()
                .report[..4],
            &[b'C', 0xff, 0xee, 0xdd]
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

    #[test]
    fn loaded_entity_supports_block_reads_and_writes() {
        let mut state = PortalState::new();
        let mut image = [0; FIGURE_IMAGE_BYTES];
        for (index, byte) in image.iter_mut().enumerate() {
            *byte = index as u8;
        }
        assert!(state.load_entity(42, &image));
        advance_slot_status(&mut state, SlotStatus::Ready);

        let query = handle_command(&mut state, &[b'Q', 0x10, 0x02]).unwrap();
        assert_eq!(&query.report[..3], &[b'Q', 0x10, 0x02]);
        assert_eq!(&query.report[3..19], &image[32..48]);

        let write_data = [0xa5; FIGURE_BLOCK_BYTES];
        let mut write_command = [0; 19];
        write_command[0] = b'W';
        write_command[1] = 0x10;
        write_command[2] = 0x02;
        write_command[3..].copy_from_slice(&write_data);

        let write = handle_command(&mut state, &write_command).unwrap();
        assert_eq!(&write.report[..3], &[b'W', 0x10, 0x02]);
        assert!(state.is_dirty());
        assert_eq!(&state.image()[32..48], &write_data);
    }

    #[test]
    fn multiple_loaded_entities_use_independent_slots() {
        let mut state = PortalState::new();
        let image_one = [0x11; FIGURE_IMAGE_BYTES];
        let image_two = [0x22; FIGURE_IMAGE_BYTES];

        assert!(state.load_entity_into_slot(0, 100, &image_one));
        assert!(state.load_entity_into_slot(1, 200, &image_two));
        advance_numbered_slot_status(&mut state, 0, SlotStatus::Ready);
        advance_numbered_slot_status(&mut state, 1, SlotStatus::Ready);

        let query_one = handle_command(&mut state, &[b'Q', 0x10, 0x02]).unwrap();
        let query_two = handle_command(&mut state, &[b'Q', 0x11, 0x02]).unwrap();

        assert_eq!(&query_one.report[..3], &[b'Q', 0x10, 0x02]);
        assert_eq!(&query_one.report[3..19], &[0x11; FIGURE_BLOCK_BYTES]);
        assert_eq!(&query_two.report[..3], &[b'Q', 0x11, 0x02]);
        assert_eq!(&query_two.report[3..19], &[0x22; FIGURE_BLOCK_BYTES]);

        let write_data = [0xa5; FIGURE_BLOCK_BYTES];
        let mut write_command = [0; 19];
        write_command[0] = b'W';
        write_command[1] = 0x11;
        write_command[2] = 0x02;
        write_command[3..].copy_from_slice(&write_data);

        let write = handle_command(&mut state, &write_command).unwrap();
        assert_eq!(&write.report[..3], &[b'W', 0x11, 0x02]);
        assert!(!state.is_slot_dirty(0));
        assert!(state.is_slot_dirty(1));
        assert_eq!(&state.slot_image(1).unwrap()[32..48], &write_data);
        assert_eq!(
            &state.slot_image(0).unwrap()[32..48],
            &[0x11; FIGURE_BLOCK_BYTES]
        );
    }

    #[test]
    fn wii_trace_empty_portal_then_player_one_placement() {
        let mut state = PortalState::new();
        let mut image = [0; FIGURE_IMAGE_BYTES];
        for (index, byte) in image.iter_mut().enumerate() {
            *byte = index as u8;
        }

        replay_trace(
            &mut state,
            &[
                TraceStep::new(&[b'A', 0x01], &[b'A', 0x01, 0xff, 0x77], true),
                TraceStep::new(&[b'R', 0x00], &[b'R', 0x02, 0x1b], true),
                TraceStep::new(&[b'S'], &[b'S', 0x00, 0x00, 0x00, 0x00, 0x00, 0x01], false),
                TraceStep::new(&[b'C', 0xff, 0xff, 0xff], &[b'C', 0xff, 0xff, 0xff], false),
            ],
        );

        assert!(state.load_entity_into_slot(0, 100, &image));
        assert_eq!(
            status_for_slot(&handle_command(&mut state, &[b'S']).unwrap().report, 0),
            SlotStatus::Added
        );
        advance_numbered_slot_status(&mut state, 0, SlotStatus::Ready);

        replay_trace(
            &mut state,
            &[
                TraceStep::new(&[b'Q', 0x10, 0x00], &[b'Q', 0x10, 0x00], true),
                TraceStep::new(&[b'Q', 0x10, 0x02], &[b'Q', 0x10, 0x02], true),
            ],
        );
        assert_eq!(
            &handle_command(&mut state, &[b'Q', 0x10, 0x02])
                .unwrap()
                .report[3..19],
            &image[32..48]
        );
    }

    #[test]
    fn wii_trace_two_player_placement_keeps_both_figures_ready() {
        let mut state = PortalState::new();
        let image_one = [0x11; FIGURE_IMAGE_BYTES];
        let image_two = [0x22; FIGURE_IMAGE_BYTES];

        replay_trace(
            &mut state,
            &[TraceStep::new(
                &[b'A', 0x01],
                &[b'A', 0x01, 0xff, 0x77],
                true,
            )],
        );

        assert!(state.load_entity_into_slot(0, 100, &image_one));
        advance_numbered_slot_status(&mut state, 0, SlotStatus::Ready);
        assert_eq!(
            status_for_slot(&state.next_status_report(), 0),
            SlotStatus::Ready
        );

        assert!(state.load_entity_into_slot(1, 200, &image_two));
        assert_eq!(
            status_for_slot(&state.next_status_report(), 1),
            SlotStatus::Added
        );
        advance_numbered_slot_status(&mut state, 1, SlotStatus::Ready);

        let status = handle_command(&mut state, &[b'S']).unwrap().report;
        assert_eq!(status_for_slot(&status, 0), SlotStatus::Ready);
        assert_eq!(status_for_slot(&status, 1), SlotStatus::Ready);

        let query_one = handle_command(&mut state, &[b'Q', 0x10, 0x00]).unwrap();
        let query_two = handle_command(&mut state, &[b'Q', 0x11, 0x00]).unwrap();
        assert_eq!(&query_one.report[..3], &[b'Q', 0x10, 0x00]);
        assert_eq!(&query_two.report[..3], &[b'Q', 0x11, 0x00]);
        assert_eq!(&query_one.report[3..19], &[0x11; FIGURE_BLOCK_BYTES]);
        assert_eq!(&query_two.report[3..19], &[0x22; FIGURE_BLOCK_BYTES]);
    }

    #[test]
    fn wii_trace_remove_and_readd_replays_physical_cycle() {
        let mut state = PortalState::new();
        let image = [0x33; FIGURE_IMAGE_BYTES];

        assert!(state.load_entity_into_slot(0, 100, &image));
        advance_numbered_slot_status(&mut state, 0, SlotStatus::Ready);

        state.clear_slot(0);
        advance_numbered_slot_status(&mut state, 0, SlotStatus::Removing);
        advance_numbered_slot_status(&mut state, 0, SlotStatus::Removed);

        assert!(state.load_entity_into_slot(0, 100, &image));
        advance_numbered_slot_status(&mut state, 0, SlotStatus::Added);
        advance_numbered_slot_status(&mut state, 0, SlotStatus::Ready);
        assert_eq!(
            &handle_command(&mut state, &[b'Q', 0x10, 0x00])
                .unwrap()
                .report[3..19],
            &[0x33; FIGURE_BLOCK_BYTES]
        );
    }

    #[test]
    fn replacing_loaded_entity_replays_physical_placement_cycle() {
        let mut state = PortalState::new();
        let image = [0; FIGURE_IMAGE_BYTES];
        assert!(state.load_entity(1, &image));
        advance_slot_status(&mut state, SlotStatus::Added);
        advance_slot_status(&mut state, SlotStatus::Ready);

        assert!(state.load_entity(1, &image));

        advance_slot_status(&mut state, SlotStatus::Removing);
        advance_slot_status(&mut state, SlotStatus::Removed);
        advance_slot_status(&mut state, SlotStatus::Added);
        advance_slot_status(&mut state, SlotStatus::Ready);
    }

    struct TraceStep<'a> {
        command: &'a [u8],
        response_prefix: &'a [u8],
        queue_report: bool,
    }

    impl<'a> TraceStep<'a> {
        const fn new(command: &'a [u8], response_prefix: &'a [u8], queue_report: bool) -> Self {
            Self {
                command,
                response_prefix,
                queue_report,
            }
        }
    }

    fn replay_trace(state: &mut PortalState, steps: &[TraceStep<'_>]) {
        for step in steps {
            let response = handle_command(state, step.command).expect("trace command is handled");
            assert_eq!(response.queue_report, step.queue_report);
            assert_eq!(
                &response.report[..step.response_prefix.len()],
                step.response_prefix,
                "unexpected response for command {:?}",
                step.command
            );
        }
    }

    fn advance_slot_status(state: &mut PortalState, expected: SlotStatus) {
        advance_numbered_slot_status(state, 0, expected);
    }

    fn advance_numbered_slot_status(state: &mut PortalState, slot: u8, expected: SlotStatus) {
        for _ in 0..=PLACEMENT_STATUS_HOLD_REPORTS {
            if status_for_slot(&state.next_status_report(), slot) == expected {
                return;
            }
        }
        panic!("slot {} status did not advance to {:?}", slot, expected);
    }

    fn status_for_slot(report: &Report, slot: u8) -> SlotStatus {
        match (u32::from_le_bytes(report[1..5].try_into().unwrap()) >> (slot * 2)) & 0x03 {
            0 => SlotStatus::Removed,
            1 => SlotStatus::Ready,
            2 => SlotStatus::Removing,
            _ => SlotStatus::Added,
        }
    }
}
