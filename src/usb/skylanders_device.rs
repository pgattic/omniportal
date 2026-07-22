use embassy_time::Instant;
use embassy_time::{Duration, Timer};
use esp_hal::{
    otg_fs::{Usb, UsbBus as EspUsbBus},
    peripherals::{GPIO19, GPIO20, USB0},
};
use static_cell::StaticCell;
use usb_device::{
    class_prelude::*,
    control::{Recipient, Request, RequestType},
    device::UsbRev,
    prelude::{StringDescriptors, UsbDeviceBuilder, UsbVidPid},
};

use crate::{
    platform::println,
    storage::{self, records::RecordId},
    usb::skylanders,
};

const REPORT_QUEUE_LEN: usize = 32;
const STORAGE_POLL_TICKS: u8 = 50;
const MODE_CHANGE_REBOOT_DELAY: Duration = Duration::from_millis(1_000);
const STORAGE_WRITE_DEBOUNCE: Duration =
    Duration::from_millis(crate::storage::wear::DEFAULT_COMMIT_DEBOUNCE_MS as u64);

pub(super) async fn run(usb0: USB0<'static>, usb_dp: GPIO20<'static>, usb_dm: GPIO19<'static>) {
    static EP_MEMORY: StaticCell<[u32; 1024]> = StaticCell::new();

    let usb = Usb::new(usb0, usb_dp, usb_dm);
    let usb_bus = EspUsbBus::new(usb, EP_MEMORY.init([0; 1024]));
    let mut class = SkylandersPortalClass::new(&usb_bus);
    println!(
        "Skylanders USB endpoints: IN=0x{:02x}, OUT=0x{:02x}",
        u8::from(class.ep_in.address()),
        u8::from(class.ep_out.address())
    );
    let mut usb_dev = UsbDeviceBuilder::new(&usb_bus, UsbVidPid(skylanders::VID, skylanders::PID))
        .strings(&[StringDescriptors::default()
            .manufacturer("OmniPortal")
            .product("Portal of Power")])
        .unwrap()
        .max_packet_size_0(64)
        .unwrap()
        .device_class(0x00)
        .device_sub_class(0x00)
        .device_protocol(0x00)
        .device_release(skylanders::BCD_DEVICE)
        .usb_rev(UsbRev::Usb200)
        .max_power(skylanders::CONFIG_MAX_POWER_MA as usize)
        .unwrap()
        .build();

    super::esp32s3::force_host_reenumeration(&mut usb_dev, "Skylanders");

    loop {
        class.poll_active_entity();
        class.flush_dirty_entity(false);

        if crate::usb::reboot_after_usb_flush_requested() {
            println!("Skylanders USB flushing active writes before mode re-enumeration");
            class.flush_dirty_entity(true);
            Timer::after(MODE_CHANGE_REBOOT_DELAY).await;
            esp_hal::system::software_reset();
        }

        if usb_dev.poll(&mut [&mut class]) {
            class.poll_usb();
        }
        class.poll_in_endpoint();

        Timer::after(Duration::from_millis(1)).await;
    }
}

struct SkylandersPortalClass<'a, B: usb_device::bus::UsbBus> {
    iface: InterfaceNumber,
    ep_in: EndpointIn<'a, B>,
    ep_out: EndpointOut<'a, B>,
    state: skylanders::PortalState,
    queue: [Option<skylanders::Report>; REPORT_QUEUE_LEN],
    out_buf: [u8; skylanders::MAX_PACKET_BYTES],
    idle_rate: u8,
    protocol: u8,
    storage_poll_ticks: u8,
    active_selection_marker: ([Option<u32>; skylanders::MAX_FIGURES], u32),
    dirty_write_deadline: Option<Instant>,
}

impl<'a, B: usb_device::bus::UsbBus> SkylandersPortalClass<'a, B> {
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
            dirty_write_deadline: None,
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
                self.handle_command_source("intr-out", &command[..count]);
            }
            Err(UsbError::WouldBlock) => {}
            Err(error) => {
                println!("Skylanders USB interrupt OUT read error: {:?}", error);
            }
        }
    }

    fn poll_in_endpoint(&mut self) {
        if let Some(report) = self.pop_report() {
            match self.ep_in.write(&report) {
                Ok(_) => {}
                Err(UsbError::WouldBlock) => {
                    self.push_report_front(report);
                }
                Err(error) => {
                    println!(
                        "Skylanders USB interrupt IN queued write error: {:?}",
                        error
                    );
                }
            }
            return;
        }

        if self.state.has_present_entities() {
            let report = self.state.next_status_report();
            match self.ep_in.write(&report) {
                Ok(_) | Err(UsbError::WouldBlock) => {}
                Err(error) => {
                    println!(
                        "Skylanders USB interrupt IN present-status write error: {:?}",
                        error
                    );
                }
            }
        }
    }

    fn handle_command_source(&mut self, source: &str, command: &[u8]) {
        if let Some(response) = skylanders::handle_command(&mut self.state, command) {
            if command.first().copied() == Some(b'W') && response.report[0] == b'W' {
                self.schedule_dirty_flush();
            }
            if response.queue_report {
                self.push_report(response.report);
            }
        } else {
            println!(
                "Skylanders USB unhandled command from {}: len={}, op=0x{:02x}",
                source,
                command.len(),
                command.first().copied().unwrap_or(0)
            );
        }
    }

    fn poll_active_entity(&mut self) {
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

                let mut loaded = 0;
                for (slot, id, image) in images {
                    if self.state.slot_entity_id(slot) == Some(id.0) {
                        continue;
                    }
                    if self.state.load_entity_into_slot(slot, id.0, &image) {
                        loaded += 1;
                        placement_changed = true;
                    } else {
                        println!(
                            "Skylanders USB rejected slot {} entity {} image length {}",
                            slot,
                            id.0,
                            image.len()
                        );
                        self.state.clear_slot(slot);
                    }
                }

                self.active_selection_marker = active_marker;
                if placement_changed {
                    self.queue_status_reports(REPORT_QUEUE_LEN);
                }
                if loaded > 0 {
                    println!("Skylanders USB loaded {} active portal slot(s)", loaded);
                }
            }
            Err(error) => {
                println!(
                    "Skylanders USB failed to read active slot state: {} ({:?})",
                    error.message(),
                    error
                );
                self.state.clear_all_entities();
                self.active_selection_marker = ([None; skylanders::MAX_FIGURES], 0);
            }
        }
    }

    fn queue_status_reports(&mut self, count: usize) {
        self.drop_queued_status_reports();
        for _ in 0..count {
            let report = self.state.next_status_report();
            if !self.push_report_if_space(report) {
                break;
            }
        }
    }

    fn schedule_dirty_flush(&mut self) {
        if self.state.is_dirty() {
            self.dirty_write_deadline = Some(Instant::now() + STORAGE_WRITE_DEBOUNCE);
        }
    }

    fn flush_dirty_entity(&mut self, force: bool) {
        if !self.state.is_dirty() {
            self.dirty_write_deadline = None;
            return;
        }

        if !force {
            match self.dirty_write_deadline {
                Some(deadline) if Instant::now() < deadline => return,
                Some(_) => {}
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
            match storage::replace_entity_blob(RecordId(id), &image) {
                Ok(()) => {
                    println!(
                        "Skylanders USB persisted writes for slot {} entity {}",
                        slot, id
                    );
                    self.state.clear_slot_dirty(slot);
                    persisted = true;
                }
                Err(error) => {
                    println!(
                        "Skylanders USB failed to persist writes for slot {} entity {}: {:?}",
                        slot, id, error
                    );
                    self.schedule_dirty_flush();
                }
            }
        }

        if persisted || !self.state.is_dirty() {
            self.dirty_write_deadline = None;
        }
    }

    fn push_report(&mut self, report: skylanders::Report) {
        if self.push_report_if_space(report) {
            return;
        }
        println!("Skylanders USB response queue full; dropping newest queued response");
        self.queue[REPORT_QUEUE_LEN - 1] = Some(report);
    }

    fn push_report_if_space(&mut self, report: skylanders::Report) -> bool {
        for slot in &mut self.queue {
            if slot.is_none() {
                *slot = Some(report);
                return true;
            }
        }
        false
    }

    fn drop_queued_status_reports(&mut self) {
        let mut compacted = [None; REPORT_QUEUE_LEN];
        let mut next = 0;
        for report in self.queue.iter().flatten().copied() {
            if report[0] != b'S' {
                compacted[next] = Some(report);
                next += 1;
            }
        }
        self.queue = compacted;
    }

    fn push_report_front(&mut self, report: skylanders::Report) {
        for index in (1..REPORT_QUEUE_LEN).rev() {
            self.queue[index] = self.queue[index - 1];
        }
        self.queue[0] = Some(report);
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

impl<B: usb_device::bus::UsbBus> UsbClass<B> for SkylandersPortalClass<'_, B> {
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
                _ => {
                    println!(
                        "Skylanders USB unhandled control IN request: type={:?}, recipient={:?}, request=0x{:02x}, value=0x{:04x}, index=0x{:04x}, len={}",
                        req.request_type,
                        req.recipient,
                        req.request,
                        req.value,
                        req.index,
                        req.length
                    );
                }
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
                    self.handle_command_source("set-report", xfer.data());
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
                _ => {
                    println!(
                        "Skylanders USB unhandled control OUT request: type={:?}, recipient={:?}, request=0x{:02x}, value=0x{:04x}, index=0x{:04x}, len={}",
                        req.request_type,
                        req.recipient,
                        req.request,
                        req.value,
                        req.index,
                        req.length
                    );
                }
            }
        }
    }
}
