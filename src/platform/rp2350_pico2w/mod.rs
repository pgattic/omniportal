pub mod board;
pub mod log;
pub mod storage_flash;

use rp235x_hal as hal;
use usb_device::{
    class_prelude::*,
    control::{Recipient, Request, RequestType},
    device::UsbRev,
    prelude::{StringDescriptors, UsbDeviceBuilder, UsbVidPid},
};

use crate::{
    figures,
    storage::{self, records::RecordId},
    usb::skylanders,
};

const XTAL_FREQ_HZ: u32 = 12_000_000;
const REPORT_QUEUE_LEN: usize = 32;
const STORAGE_POLL_TICKS: u16 = 4_000;
const STORAGE_WRITE_DEBOUNCE_TICKS: u16 = 8_000;

pub fn run() -> ! {
    let mut pac = hal::pac::Peripherals::take().unwrap();
    let mut watchdog = hal::Watchdog::new(pac.WATCHDOG);
    let clocks = hal::clocks::init_clocks_and_plls(
        XTAL_FREQ_HZ,
        pac.XOSC,
        pac.CLOCKS,
        pac.PLL_SYS,
        pac.PLL_USB,
        &mut pac.RESETS,
        &mut watchdog,
    )
    .ok()
    .unwrap();

    figures::initialize();
    storage::init();

    let usb_bus = UsbBusAllocator::new(hal::usb::UsbBus::new(
        pac.USB,
        pac.USB_DPRAM,
        clocks.usb_clock,
        true,
        &mut pac.RESETS,
    ));
    let mut class = PicoSkylandersPortalClass::new(&usb_bus);
    let mut usb_dev = UsbDeviceBuilder::new(&usb_bus, UsbVidPid(skylanders::VID, skylanders::PID))
        .strings(&[StringDescriptors::default()
            .manufacturer("OmniPortal")
            .product("Portal of Power")])
        .unwrap()
        .max_packet_size_0(skylanders::DEVICE_MAX_PACKET_SIZE)
        .unwrap()
        .device_class(0x00)
        .device_sub_class(0x00)
        .device_protocol(0x00)
        .device_release(skylanders::BCD_DEVICE)
        .usb_rev(UsbRev::Usb200)
        .max_power(skylanders::CONFIG_MAX_POWER_MA as usize)
        .unwrap()
        .build();

    loop {
        if usb_dev.poll(&mut [&mut class]) {
            class.poll_usb();
        }
        class.poll_active_entity();
        class.flush_dirty_entity(false);
        class.poll_in_endpoint();
        cortex_m::asm::delay(1_000);
    }
}

struct PicoSkylandersPortalClass<'a, B: usb_device::bus::UsbBus> {
    iface: InterfaceNumber,
    ep_in: EndpointIn<'a, B>,
    ep_out: EndpointOut<'a, B>,
    state: skylanders::PortalState,
    queue: [Option<skylanders::Report>; REPORT_QUEUE_LEN],
    out_buf: [u8; skylanders::MAX_PACKET_BYTES],
    idle_rate: u8,
    protocol: u8,
    storage_poll_ticks: u16,
    active_selection_marker: ([Option<u32>; skylanders::MAX_FIGURES], u32),
    dirty_write_ticks: Option<u16>,
    activate_ack_sent: bool,
}

impl<'a, B: usb_device::bus::UsbBus> PicoSkylandersPortalClass<'a, B> {
    fn new(alloc: &'a UsbBusAllocator<B>) -> Self {
        Self {
            iface: alloc.interface(),
            ep_in: alloc
                .alloc(
                    Some(EndpointAddress::from(skylanders::INTERRUPT_IN_ENDPOINT)),
                    EndpointType::Interrupt,
                    skylanders::INTERRUPT_MAX_PACKET_SIZE,
                    skylanders::INTERRUPT_POLL_INTERVAL_MS,
                )
                .expect("alloc Skylanders interrupt IN endpoint failed"),
            ep_out: alloc
                .alloc(
                    Some(EndpointAddress::from(skylanders::INTERRUPT_OUT_ENDPOINT)),
                    EndpointType::Interrupt,
                    skylanders::INTERRUPT_MAX_PACKET_SIZE,
                    skylanders::INTERRUPT_POLL_INTERVAL_MS,
                )
                .expect("alloc Skylanders interrupt OUT endpoint failed"),
            state: skylanders::PortalState::new(),
            queue: [None; REPORT_QUEUE_LEN],
            out_buf: [0; skylanders::MAX_PACKET_BYTES],
            idle_rate: 0,
            protocol: 1,
            storage_poll_ticks: 0,
            active_selection_marker: ([None; skylanders::MAX_FIGURES], 0),
            dirty_write_ticks: None,
            activate_ack_sent: false,
        }
    }

    fn poll_usb(&mut self) {
        self.poll_out_endpoint();
    }

    fn poll_out_endpoint(&mut self) {
        match self.ep_out.read(&mut self.out_buf) {
            Ok(count) => {
                let mut command = [0; skylanders::MAX_PACKET_BYTES];
                command[..count].copy_from_slice(&self.out_buf[..count]);
                self.handle_command(&command[..count]);
            }
            Err(UsbError::WouldBlock) => {}
            Err(_) => {}
        }
    }

    fn poll_in_endpoint(&mut self) {
        if let Some(report) = self.pop_report() {
            match self.ep_in.write(&report) {
                Ok(_) => {}
                Err(UsbError::WouldBlock) => self.push_report_front(report),
                Err(_) => {}
            }
            return;
        }

        if self.state.has_present_entities() {
            let report = self.state.next_status_report();
            let _ = self.ep_in.write(&report);
        }
    }

    fn handle_command(&mut self, command: &[u8]) {
        let suppress_duplicate_activate_ack =
            matches!(command, [b'A', active, ..] if *active != 0) && self.activate_ack_sent;
        self.handle_activation_state(command);
        if let Some(response) = skylanders::handle_command(&mut self.state, command) {
            if command.first().copied() == Some(b'W')
                && response.report[0] == b'W'
                && response.report[1] >= skylanders::FIRST_FIGURE_SLOT_ID
            {
                self.schedule_dirty_flush();
            }
            if matches!(command, [b'A', active, ..] if *active != 0) && response.queue_report {
                self.activate_ack_sent = true;
            }
            if response.queue_report && !suppress_duplicate_activate_ack {
                self.push_command_response(response.report);
            }
        }
    }

    fn handle_activation_state(&mut self, command: &[u8]) {
        match command {
            [b'A', active, ..] if *active == 0 => {
                self.activate_ack_sent = false;
                self.active_selection_marker = ([None; skylanders::MAX_FIGURES], 0);
            }
            _ => {}
        }
    }

    fn poll_active_entity(&mut self) {
        if !self.state.active {
            return;
        }

        if self.storage_poll_ticks > 0 {
            self.storage_poll_ticks -= 1;
            return;
        }
        self.storage_poll_ticks = STORAGE_POLL_TICKS;

        let (active_slots, active_generation) = storage::active_slots_marker();
        let active_marker = (active_slots.map(|id| id.map(|id| id.0)), active_generation);
        if active_marker == self.active_selection_marker {
            return;
        }

        self.flush_dirty_entity(true);
        match storage::active_slot_images() {
            Ok(images) => {
                let mut placement_changed = false;
                for slot in 0..skylanders::MAX_FIGURES {
                    if active_marker.0[slot].is_none()
                        && self.state.slot_entity_id(slot as u8).is_some()
                    {
                        self.state.clear_slot(slot as u8);
                        placement_changed = true;
                    }
                }

                for (slot, id, image) in images {
                    if self.state.slot_entity_id(slot) == Some(id.0) {
                        continue;
                    }
                    if self.state.load_entity_into_slot(slot, id.0, &image) {
                        placement_changed = true;
                    } else {
                        self.state.clear_slot(slot);
                    }
                }

                self.active_selection_marker = active_marker;
                if placement_changed {
                    self.queue_status_reports(REPORT_QUEUE_LEN);
                }
            }
            Err(_) => {
                self.state.clear_all_entities();
                self.active_selection_marker = ([None; skylanders::MAX_FIGURES], 0);
            }
        }
    }

    fn queue_status_reports(&mut self, count: usize) {
        for _ in 0..count {
            let report = self.state.next_status_report();
            self.push_report(report);
        }
    }

    fn schedule_dirty_flush(&mut self) {
        if self.state.is_dirty() {
            self.dirty_write_ticks = Some(STORAGE_WRITE_DEBOUNCE_TICKS);
        }
    }

    fn flush_dirty_entity(&mut self, force: bool) {
        if !self.state.is_dirty() {
            self.dirty_write_ticks = None;
            return;
        }

        if !force {
            match self.dirty_write_ticks {
                Some(0) => {}
                Some(ticks) => {
                    self.dirty_write_ticks = Some(ticks - 1);
                    return;
                }
                None => {
                    self.schedule_dirty_flush();
                    return;
                }
            }
        }

        let mut persisted = false;
        for slot in 0..skylanders::MAX_FIGURES {
            let slot = slot as u8;
            if !self.state.is_slot_dirty(slot) {
                continue;
            }
            let Some(id) = self.state.slot_entity_id(slot) else {
                self.state.clear_slot_dirty(slot);
                continue;
            };
            let Some(image) = self.state.slot_image(slot).copied() else {
                self.state.clear_slot_dirty(slot);
                continue;
            };
            if storage::replace_entity_blob(RecordId(id), &image).is_ok() {
                self.state.clear_slot_dirty(slot);
                persisted = true;
            }
        }

        if persisted || !self.state.is_dirty() {
            self.dirty_write_ticks = None;
        }
    }

    fn push_report(&mut self, report: skylanders::Report) {
        if let Some(slot) = self.queue.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(report);
            return;
        }
        self.queue[REPORT_QUEUE_LEN - 1] = Some(report);
    }

    fn push_report_front(&mut self, report: skylanders::Report) {
        for index in (1..REPORT_QUEUE_LEN).rev() {
            self.queue[index] = self.queue[index - 1];
        }
        self.queue[0] = Some(report);
    }

    fn push_command_response(&mut self, report: skylanders::Report) {
        if self.queue[0].is_none() {
            self.queue[0] = Some(report);
        } else {
            self.push_report_front(report);
        }
    }

    fn pop_report(&mut self) -> Option<skylanders::Report> {
        let report = self.queue[0]?;
        for index in 1..REPORT_QUEUE_LEN {
            self.queue[index - 1] = self.queue[index];
        }
        self.queue[REPORT_QUEUE_LEN - 1] = None;
        Some(report)
    }

    fn is_interface_request(&self, req: &Request) -> bool {
        req.index as u8 == u8::from(self.iface)
    }
}

impl<B: usb_device::bus::UsbBus> UsbClass<B> for PicoSkylandersPortalClass<'_, B> {
    fn get_configuration_descriptors(
        &self,
        writer: &mut DescriptorWriter,
    ) -> usb_device::Result<()> {
        writer.interface(self.iface, skylanders::HID_CLASS, 0x00, 0x00)?;
        writer.write(
            skylanders::HID_DESCRIPTOR_TYPE,
            &[
                0x11,
                0x01,
                0x00,
                0x01,
                skylanders::HID_REPORT_DESCRIPTOR_TYPE,
                skylanders::HID_REPORT_DESCRIPTOR.len() as u8,
                (skylanders::HID_REPORT_DESCRIPTOR.len() >> 8) as u8,
            ],
        )?;
        writer.endpoint(&self.ep_in)?;
        writer.endpoint(&self.ep_out)?;
        Ok(())
    }

    fn control_in(&mut self, xfer: ControlIn<B>) {
        let req = *xfer.request();
        if !self.is_interface_request(&req) {
            return;
        }

        if req.request_type == RequestType::Standard
            && req.recipient == Recipient::Interface
            && req.request == Request::GET_DESCRIPTOR
            && req.descriptor_type_index().0 == skylanders::HID_REPORT_DESCRIPTOR_TYPE
        {
            let _ = xfer.accept_with_static(skylanders::HID_REPORT_DESCRIPTOR);
            return;
        }

        if req.request_type == RequestType::Class && req.recipient == Recipient::Interface {
            match req.request {
                skylanders::HID_GET_REPORT_REQUEST => {
                    let report = self
                        .pop_report()
                        .unwrap_or_else(|| self.state.next_status_report());
                    let _ = xfer.accept_with(&report);
                }
                skylanders::HID_GET_IDLE_REQUEST => {
                    let _ = xfer.accept_with(&[self.idle_rate]);
                }
                skylanders::HID_GET_PROTOCOL_REQUEST => {
                    let _ = xfer.accept_with(&[self.protocol]);
                }
                _ => {}
            }
        }
    }

    fn control_out(&mut self, xfer: ControlOut<B>) {
        let req = *xfer.request();
        if !self.is_interface_request(&req) {
            return;
        }

        if req.request_type == RequestType::Class && req.recipient == Recipient::Interface {
            match req.request {
                skylanders::HID_SET_REPORT_REQUEST => {
                    self.handle_command(xfer.data());
                    let _ = xfer.accept();
                }
                skylanders::HID_SET_IDLE_REQUEST => {
                    self.idle_rate = (req.value >> 8) as u8;
                    let _ = xfer.accept();
                }
                skylanders::HID_SET_PROTOCOL_REQUEST => {
                    self.protocol = req.value as u8;
                    let _ = xfer.accept();
                }
                _ => {}
            }
        }
    }
}
