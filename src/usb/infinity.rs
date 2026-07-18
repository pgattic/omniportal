pub const VID: u16 = 0x0e6f;
pub const PID: u16 = 0x0129;

pub const BCD_USB: u16 = 0x0200;
pub const BCD_DEVICE: u16 = 0x0200;
pub const DEVICE_MAX_PACKET_SIZE: u8 = 32;
pub const CONFIG_TOTAL_LENGTH: u16 = 0x0029;
pub const CONFIG_MAX_POWER_MA: u16 = 500;
pub const INTERFACE_NUMBER: u8 = 0;
pub const HID_CLASS: u8 = 0x03;
pub const INTERRUPT_IN_ENDPOINT: u8 = 0x81;
pub const INTERRUPT_OUT_ENDPOINT: u8 = 0x01;
pub const INTERRUPT_MAX_PACKET_SIZE: u16 = 32;
pub const INTERRUPT_POLL_INTERVAL_MS: u8 = 1;

pub const HID_GET_REPORT_REQUEST: u8 = 0x01;
pub const HID_GET_IDLE_REQUEST: u8 = 0x02;
pub const HID_GET_PROTOCOL_REQUEST: u8 = 0x03;
pub const HID_SET_REPORT_REQUEST: u8 = 0x09;
pub const HID_SET_IDLE_REQUEST: u8 = 0x0a;
pub const HID_SET_PROTOCOL_REQUEST: u8 = 0x0b;
pub const HID_DESCRIPTOR_TYPE: u8 = 0x21;
pub const HID_REPORT_DESCRIPTOR_TYPE: u8 = 0x22;

pub const REPORT_BYTES: usize = 32;
pub const MAX_FIGURES: usize = 9;
pub const FIGURE_BLOCK_BYTES: usize = 16;
pub const FIGURE_BLOCK_COUNT: usize = 0x14;
pub const FIGURE_IMAGE_BYTES: usize = FIGURE_BLOCK_BYTES * FIGURE_BLOCK_COUNT;

pub type Report = [u8; REPORT_BYTES];
pub type FigureImage = [u8; FIGURE_IMAGE_BYTES];

pub const ACTIVATE_RESPONSE: Report = [
    0xaa, 0x15, 0x00, 0x00, 0x0f, 0x01, 0x00, 0x03, 0x02, 0x09, 0x09, 0x43, 0x20, 0x32, 0x62, 0x36,
    0x36, 0x4b, 0x34, 0x99, 0x67, 0x31, 0x93, 0x8c, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

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
#[repr(u8)]
pub enum FigurePosition {
    HexagonDiscOne = 0,
    HexagonDiscTwo = 1,
    HexagonDiscThree = 2,
    PlayerOne = 3,
    PlayerOneAbilityOne = 4,
    PlayerOneAbilityTwo = 5,
    PlayerTwo = 6,
    PlayerTwoAbilityOne = 7,
    PlayerTwoAbilityTwo = 8,
}

impl FigurePosition {
    const fn index(self) -> usize {
        self as usize
    }

    pub const fn from_portal_index(index: u8) -> Option<Self> {
        match index {
            0 => Some(Self::PlayerOne),
            1 => Some(Self::PlayerTwo),
            2 => Some(Self::HexagonDiscOne),
            3 => Some(Self::PlayerOneAbilityOne),
            4 => Some(Self::PlayerOneAbilityTwo),
            5 => Some(Self::PlayerTwoAbilityOne),
            6 => Some(Self::PlayerTwoAbilityTwo),
            7 => Some(Self::HexagonDiscTwo),
            8 => Some(Self::HexagonDiscThree),
            _ => None,
        }
    }

    pub const fn portal_index(self) -> u8 {
        match self {
            Self::PlayerOne => 0,
            Self::PlayerTwo => 1,
            Self::HexagonDiscOne => 2,
            Self::PlayerOneAbilityOne => 3,
            Self::PlayerOneAbilityTwo => 4,
            Self::PlayerTwoAbilityOne => 5,
            Self::PlayerTwoAbilityTwo => 6,
            Self::HexagonDiscTwo => 7,
            Self::HexagonDiscThree => 8,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum BasePosition {
    Unknown = 0,
    Hexagon = 1,
    PlayerOne = 2,
    PlayerTwo = 3,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FigureSlot {
    present: bool,
    order_added: u8,
    data: FigureImage,
}

impl FigureSlot {
    const fn new() -> Self {
        Self {
            present: false,
            order_added: 0xff,
            data: [0; FIGURE_IMAGE_BYTES],
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InfinityBaseState {
    figures: [FigureSlot; MAX_FIGURES],
    next_order: u8,
    queued_changes: [Option<Report>; MAX_FIGURES * 2],
    random_a: u32,
    random_b: u32,
    random_c: u32,
    random_d: u32,
}

impl InfinityBaseState {
    pub const fn new() -> Self {
        Self {
            figures: [FigureSlot::new(); MAX_FIGURES],
            next_order: 0,
            queued_changes: [None; MAX_FIGURES * 2],
            random_a: 0,
            random_b: 0,
            random_c: 0,
            random_d: 0,
        }
    }

    pub fn load_figure(&mut self, position: FigurePosition, image: &[u8]) -> bool {
        if image.len() != FIGURE_IMAGE_BYTES {
            return false;
        }
        let slot = &mut self.figures[position.index()];
        slot.data.copy_from_slice(image);
        slot.present = true;
        if slot.order_added == 0xff {
            slot.order_added = self.next_order;
            self.next_order = self.next_order.wrapping_add(1);
        }
        self.push_change_response(position, false);
        true
    }

    pub fn remove_figure(&mut self, position: FigurePosition) {
        if !self.figures[position.index()].present {
            return;
        }
        self.figures[position.index()].present = false;
        self.push_change_response(position, true);
    }

    pub fn has_change_response(&self) -> bool {
        self.queued_changes.iter().any(Option::is_some)
    }

    pub fn pop_change_response(&mut self) -> Option<Report> {
        pop_report(&mut self.queued_changes)
    }

    pub fn present_figures_response(&self, sequence: u8) -> Report {
        let mut report = [0; REPORT_BYTES];
        let mut cursor = 3usize;
        for (index, figure) in self.figures.iter().enumerate() {
            if !figure.present || cursor + 1 >= REPORT_BYTES {
                continue;
            }
            let Some(position) = figure_position_from_index(index) else {
                continue;
            };
            report[cursor] = presence_position_byte(position).wrapping_add(figure.order_added);
            report[cursor + 1] = 0x09;
            cursor += 2;
        }
        report[0] = 0xaa;
        report[1] = cursor.saturating_sub(2) as u8;
        report[2] = sequence;
        report[cursor] = checksum(&report, cursor);
        report
    }

    pub fn figure_identifier_response(&self, order_added: u8, sequence: u8) -> Report {
        let mut report = [0; REPORT_BYTES];
        report[0] = 0xaa;
        report[1] = 0x09;
        report[2] = sequence;
        if let Some(figure) = self.figure_by_order(order_added) {
            report[4..11].copy_from_slice(&figure.data[..7]);
        }
        report[11] = checksum(&report, 11);
        report
    }

    pub fn query_block_response(&self, order_added: u8, block: u8, sequence: u8) -> Report {
        let mut report = [0; REPORT_BYTES];
        report[0] = 0xaa;
        report[1] = 0x12;
        report[2] = sequence;
        if let Some(figure) = self.figure_by_order(order_added) {
            if let Some(file_block) = command_block_to_file_block(block) {
                let offset = file_block as usize * FIGURE_BLOCK_BYTES;
                report[4..20].copy_from_slice(&figure.data[offset..offset + FIGURE_BLOCK_BYTES]);
            }
        }
        report[20] = checksum(&report, 20);
        report
    }

    pub fn write_block_response(
        &mut self,
        order_added: u8,
        block: u8,
        data: &[u8],
        sequence: u8,
    ) -> Report {
        let mut report = [0; REPORT_BYTES];
        report[0] = 0xaa;
        report[1] = 0x02;
        report[2] = sequence;
        if data.len() >= FIGURE_BLOCK_BYTES {
            if let Some(figure) = self.figure_by_order_mut(order_added) {
                if let Some(file_block) = command_block_to_file_block(block) {
                    let offset = file_block as usize * FIGURE_BLOCK_BYTES;
                    figure.data[offset..offset + FIGURE_BLOCK_BYTES]
                        .copy_from_slice(&data[..FIGURE_BLOCK_BYTES]);
                }
            }
        }
        report[4] = checksum(&report, 4);
        report
    }

    pub fn descramble_and_seed_response(&mut self, packet: &Report, sequence: u8) -> Report {
        let scrambled = u64::from_be_bytes([
            packet[4], packet[5], packet[6], packet[7], packet[8], packet[9], packet[10],
            packet[11],
        ]);
        self.generate_seed(descramble(scrambled));
        blank_response(sequence)
    }

    pub fn next_random_response(&mut self, sequence: u8) -> Report {
        let scrambled = scramble(self.next_random(), 0);
        let bytes = scrambled.to_be_bytes();
        let mut report = [0; REPORT_BYTES];
        report[0] = 0xaa;
        report[1] = 0x09;
        report[2] = sequence;
        report[3..11].copy_from_slice(&bytes);
        report[11] = checksum(&report, 11);
        report
    }

    pub fn figure_image(&self, position: FigurePosition) -> Option<&FigureImage> {
        let figure = &self.figures[position.index()];
        figure.present.then_some(&figure.data)
    }

    pub fn figure_position_by_order(&self, order_added: u8) -> Option<FigurePosition> {
        self.figures
            .iter()
            .enumerate()
            .find(|(_, figure)| figure.present && figure.order_added == order_added)
            .and_then(|(index, _)| figure_position_from_index(index))
    }

    fn push_change_response(&mut self, position: FigurePosition, removed: bool) {
        let mut report = [0; REPORT_BYTES];
        let figure = &self.figures[position.index()];
        report[0] = 0xab;
        report[1] = 0x04;
        report[2] = derive_base_position(position) as u8;
        report[3] = 0x09;
        report[4] = figure.order_added;
        report[5] = u8::from(removed);
        report[6] = checksum(&report, 6);
        push_report(&mut self.queued_changes, report);
    }

    fn figure_by_order(&self, order_added: u8) -> Option<&FigureSlot> {
        self.figures
            .iter()
            .find(|figure| figure.present && figure.order_added == order_added)
    }

    fn figure_by_order_mut(&mut self, order_added: u8) -> Option<&mut FigureSlot> {
        self.figures
            .iter_mut()
            .find(|figure| figure.present && figure.order_added == order_added)
    }

    fn generate_seed(&mut self, seed: u32) {
        self.random_a = 0xf1ea_5eed;
        self.random_b = seed;
        self.random_c = seed;
        self.random_d = seed;

        for _ in 0..23 {
            self.next_random();
        }
    }

    fn next_random(&mut self) -> u32 {
        let mut a = self.random_a;
        let mut b = self.random_b;
        let mut c = self.random_c;
        let ret = self.random_b.rotate_left(27);

        let temp = a.wrapping_add((ret ^ 0xffff_ffff).wrapping_add(1));
        b ^= c.rotate_left(17);
        a = self.random_d;
        c = c.wrapping_add(a);
        let ret = b.wrapping_add(temp);
        a = a.wrapping_add(temp);

        self.random_c = a;
        self.random_a = b;
        self.random_b = c;
        self.random_d = ret;

        ret
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommandResponse {
    pub echo: Report,
    pub response: Report,
}

pub fn handle_command_packet(
    state: &mut InfinityBaseState,
    packet: &Report,
) -> Option<CommandResponse> {
    if packet[0] != 0xff || !valid_command_packet(packet) {
        return None;
    }
    let command = packet[2];
    let sequence = packet[3];
    let response = match command {
        0x80 => ACTIVATE_RESPONSE,
        0x81 => state.descramble_and_seed_response(packet, sequence),
        0x90 | 0x92 | 0x93 | 0x95 | 0x96 | 0xb5 => blank_response(sequence),
        0x83 => state.next_random_response(sequence),
        0xa1 => state.present_figures_response(sequence),
        0xa2 => state.query_block_response(packet[4], packet[5], sequence),
        0xa3 => state.write_block_response(packet[4], packet[5], &packet[7..23], sequence),
        0xb4 => state.figure_identifier_response(packet[4], sequence),
        _ => blank_response(sequence),
    };
    Some(CommandResponse {
        echo: *packet,
        response,
    })
}

pub fn blank_response(sequence: u8) -> Report {
    let mut report = [0; REPORT_BYTES];
    report[0] = 0xaa;
    report[1] = 0x01;
    report[2] = sequence;
    report[3] = checksum(&report, 3);
    report
}

pub fn scramble(mut num_to_scramble: u32, mut garbage: u32) -> u64 {
    let mut mask = 0x8e55_aa1b_3999_e8aau64;
    let mut ret = 0u64;

    for _ in 0..64 {
        ret <<= 1;
        if mask & 1 != 0 {
            ret |= u64::from(num_to_scramble & 1);
            num_to_scramble >>= 1;
        } else {
            ret |= u64::from(garbage & 1);
            garbage >>= 1;
        }
        mask >>= 1;
    }

    ret
}

pub fn descramble(mut num_to_descramble: u64) -> u32 {
    let mut mask = 0x8e55_aa1b_3999_e8aau64;
    let mut ret = 0u32;
    let mut ret_mask = 0x8000_0000u32;

    for _ in 0..64 {
        if mask & 0x8000_0000_0000_0000 != 0 {
            ret |= ((num_to_descramble & 1) as u32) * ret_mask;
            ret_mask >>= 1;
        }
        num_to_descramble >>= 1;
        mask <<= 1;
    }

    ret
}

pub fn checksum(report: &Report, bytes: usize) -> u8 {
    report
        .iter()
        .take(bytes)
        .fold(0u8, |sum, byte| sum.wrapping_add(*byte))
}

pub fn valid_command_packet(packet: &Report) -> bool {
    let len = packet[1] as usize;
    let checksum_index = 2 + len;
    checksum_index < REPORT_BYTES && packet[checksum_index] == checksum(packet, checksum_index)
}

pub fn command_packet(command: u8, sequence: u8, payload: &[u8]) -> Option<Report> {
    let len = payload.len().checked_add(2)?;
    let checksum_index = 2usize.checked_add(len)?;
    if checksum_index >= REPORT_BYTES {
        return None;
    }
    let mut packet = [0; REPORT_BYTES];
    packet[0] = 0xff;
    packet[1] = len as u8;
    packet[2] = command;
    packet[3] = sequence;
    packet[4..4 + payload.len()].copy_from_slice(payload);
    packet[checksum_index] = checksum(&packet, checksum_index);
    Some(packet)
}

pub const fn derive_base_position(position: FigurePosition) -> BasePosition {
    match position {
        FigurePosition::HexagonDiscOne
        | FigurePosition::HexagonDiscTwo
        | FigurePosition::HexagonDiscThree => BasePosition::Hexagon,
        FigurePosition::PlayerOne
        | FigurePosition::PlayerOneAbilityOne
        | FigurePosition::PlayerOneAbilityTwo => BasePosition::PlayerOne,
        FigurePosition::PlayerTwo
        | FigurePosition::PlayerTwoAbilityOne
        | FigurePosition::PlayerTwoAbilityTwo => BasePosition::PlayerTwo,
    }
}

pub const fn presence_position_byte(position: FigurePosition) -> u8 {
    match derive_base_position(position) {
        BasePosition::Hexagon => 0x10,
        BasePosition::PlayerOne => 0x20,
        BasePosition::PlayerTwo => 0x30,
        BasePosition::Unknown => 0x00,
    }
}

fn command_block_to_file_block(block: u8) -> Option<u8> {
    let file_block = if block == 0 {
        1
    } else {
        block.saturating_mul(4)
    };
    (file_block < FIGURE_BLOCK_COUNT as u8).then_some(file_block)
}

fn figure_position_from_index(index: usize) -> Option<FigurePosition> {
    match index {
        0 => Some(FigurePosition::HexagonDiscOne),
        1 => Some(FigurePosition::HexagonDiscTwo),
        2 => Some(FigurePosition::HexagonDiscThree),
        3 => Some(FigurePosition::PlayerOne),
        4 => Some(FigurePosition::PlayerOneAbilityOne),
        5 => Some(FigurePosition::PlayerOneAbilityTwo),
        6 => Some(FigurePosition::PlayerTwo),
        7 => Some(FigurePosition::PlayerTwoAbilityOne),
        8 => Some(FigurePosition::PlayerTwoAbilityTwo),
        _ => None,
    }
}

fn push_report(queue: &mut [Option<Report>; MAX_FIGURES * 2], report: Report) {
    if let Some(slot) = queue.iter_mut().find(|slot| slot.is_none()) {
        *slot = Some(report);
    }
}

fn pop_report(queue: &mut [Option<Report>; MAX_FIGURES * 2]) -> Option<Report> {
    let report = queue[0]?;
    for index in 1..queue.len() {
        queue[index - 1] = queue[index];
    }
    queue[queue.len() - 1] = None;
    Some(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_constants_match_dolphin_reference() {
        assert_eq!(VID, 0x0e6f);
        assert_eq!(PID, 0x0129);
        assert_eq!(DEVICE_MAX_PACKET_SIZE, 32);
        assert_eq!(CONFIG_TOTAL_LENGTH, 0x0029);
        assert_eq!(INTERRUPT_IN_ENDPOINT, 0x81);
        assert_eq!(INTERRUPT_OUT_ENDPOINT, 0x01);
        assert_eq!(INTERRUPT_MAX_PACKET_SIZE, 32);
    }

    #[test]
    fn command_packet_checksum_matches_wire_format() {
        let packet = command_packet(0xa1, 0x42, &[]).unwrap();

        assert_eq!(&packet[..5], &[0xff, 0x02, 0xa1, 0x42, 0xe4]);
        assert!(valid_command_packet(&packet));
    }

    #[test]
    fn random_seed_commands_follow_dolphin_scramble_flow() {
        for value in [0, 1, 0x1234_5678, 0x9eb3_cd88, u32::MAX] {
            assert_eq!(descramble(scramble(value, 0)), value);
        }

        let mut state = InfinityBaseState::new();
        let seed = scramble(0x1234_5678, 0).to_be_bytes();
        let seed_response =
            handle_command_packet(&mut state, &command_packet(0x81, 0x10, &seed).unwrap()).unwrap();
        assert_eq!(&seed_response.response[..4], &[0xaa, 0x01, 0x10, 0xbb]);

        let random_response =
            handle_command_packet(&mut state, &command_packet(0x83, 0x11, &[]).unwrap()).unwrap();
        assert_eq!(&random_response.response[..3], &[0xaa, 0x09, 0x11]);
        assert_ne!(&random_response.response[3..11], &[0; 8]);
        assert_eq!(
            random_response.response[11],
            checksum(&random_response.response, 11)
        );
    }

    #[test]
    fn activate_and_blank_commands_return_expected_frames() {
        let mut state = InfinityBaseState::new();

        let activate =
            handle_command_packet(&mut state, &command_packet(0x80, 0x01, &[]).unwrap()).unwrap();
        assert_eq!(activate.response, ACTIVATE_RESPONSE);

        let blank =
            handle_command_packet(&mut state, &command_packet(0xb5, 0x09, &[]).unwrap()).unwrap();
        assert_eq!(&blank.response[..4], &[0xaa, 0x01, 0x09, 0xb4]);
    }

    #[test]
    fn present_figures_reports_order_and_base_position() {
        let mut state = InfinityBaseState::new();
        let image_one = [0x11; FIGURE_IMAGE_BYTES];
        let image_two = [0x22; FIGURE_IMAGE_BYTES];

        assert!(state.load_figure(FigurePosition::PlayerOne, &image_one));
        assert!(state.load_figure(FigurePosition::PlayerTwo, &image_two));

        let report = state.present_figures_response(0x12);
        assert_eq!(
            &report[..8],
            &[0xaa, 0x05, 0x12, 0x20, 0x09, 0x31, 0x09, 0x24]
        );
    }

    #[test]
    fn add_and_remove_queue_change_reports() {
        let mut state = InfinityBaseState::new();
        assert!(state.load_figure(FigurePosition::HexagonDiscOne, &[0x33; FIGURE_IMAGE_BYTES]));

        assert!(state.has_change_response());
        assert_eq!(
            &state.pop_change_response().unwrap()[..7],
            &[0xab, 0x04, 0x01, 0x09, 0x00, 0x00, 0xb9]
        );

        state.remove_figure(FigurePosition::HexagonDiscOne);
        assert_eq!(
            &state.pop_change_response().unwrap()[..7],
            &[0xab, 0x04, 0x01, 0x09, 0x00, 0x01, 0xba]
        );
    }

    #[test]
    fn tag_identifier_and_block_io_use_order_added() {
        let mut state = InfinityBaseState::new();
        let mut image = [0; FIGURE_IMAGE_BYTES];
        for (index, byte) in image.iter_mut().enumerate() {
            *byte = index as u8;
        }
        assert!(state.load_figure(FigurePosition::PlayerOne, &image));

        let id = state.figure_identifier_response(0, 0x33);
        assert_eq!(
            &id[..12],
            &[0xaa, 0x09, 0x33, 0x00, 0, 1, 2, 3, 4, 5, 6, 0xfb]
        );

        let read = state.query_block_response(0, 0x02, 0x34);
        assert_eq!(&read[..4], &[0xaa, 0x12, 0x34, 0x00]);
        assert_eq!(
            &read[4..20],
            &image[8 * FIGURE_BLOCK_BYTES..9 * FIGURE_BLOCK_BYTES]
        );

        let write_data = [0xa5; FIGURE_BLOCK_BYTES];
        let write = state.write_block_response(0, 0x02, &write_data, 0x35);
        assert_eq!(&write[..5], &[0xaa, 0x02, 0x35, 0x00, 0xe1]);
        assert_eq!(
            &state.figure_image(FigurePosition::PlayerOne).unwrap()
                [8 * FIGURE_BLOCK_BYTES..9 * FIGURE_BLOCK_BYTES],
            &write_data
        );
    }
}
