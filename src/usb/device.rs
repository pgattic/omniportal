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
    domain::GameLine,
    platform::println,
    storage::{self, records::RecordId},
    usb::{infinity, skylanders},
};

const REPORT_QUEUE_LEN: usize = 32;
const STORAGE_POLL_TICKS: u8 = 50;
const INFINITY_CHANGE_REPORT_REPEATS: usize = 4;
const STORAGE_WRITE_DEBOUNCE: Duration =
    Duration::from_millis(crate::storage::wear::DEFAULT_COMMIT_DEBOUNCE_MS as u64);

#[embassy_executor::task]
pub async fn run(usb0: USB0<'static>, usb_dp: GPIO20<'static>, usb_dm: GPIO19<'static>) {
    match storage::usb_mode() {
        GameLine::Skylanders => run_skylanders(usb0, usb_dp, usb_dm).await,
        GameLine::Infinity => run_infinity(usb0, usb_dp, usb_dm).await,
    }
}

async fn run_skylanders(usb0: USB0<'static>, usb_dp: GPIO20<'static>, usb_dm: GPIO19<'static>) {
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

    loop {
        class.poll_active_entity();
        class.flush_dirty_entity(false);

        if usb_dev.poll(&mut [&mut class]) {
            class.poll_usb();
        }
        class.poll_in_endpoint();

        Timer::after(Duration::from_millis(1)).await;
    }
}

async fn run_infinity(usb0: USB0<'static>, usb_dp: GPIO20<'static>, usb_dm: GPIO19<'static>) {
    static EP_MEMORY: StaticCell<[u32; 1024]> = StaticCell::new();

    let usb = Usb::new(usb0, usb_dp, usb_dm);
    let usb_bus = EspUsbBus::new(usb, EP_MEMORY.init([0; 1024]));
    let mut class = InfinityBaseClass::new(&usb_bus);
    println!(
        "Disney Infinity USB endpoints: IN=0x{:02x}, OUT=0x{:02x}",
        u8::from(class.ep_in.address()),
        u8::from(class.ep_out.address())
    );
    let mut usb_dev = UsbDeviceBuilder::new(&usb_bus, UsbVidPid(infinity::VID, infinity::PID))
        .strings(&[StringDescriptors::default()
            .manufacturer("OmniPortal")
            .product("Disney Infinity Base")])
        .unwrap()
        .max_packet_size_0(infinity::DEVICE_MAX_PACKET_SIZE)
        .unwrap()
        .device_class(0x00)
        .device_sub_class(0x00)
        .device_protocol(0x00)
        .device_release(infinity::BCD_DEVICE)
        .usb_rev(UsbRev::Usb200)
        .max_power(infinity::CONFIG_MAX_POWER_MA as usize)
        .unwrap()
        .build();

    loop {
        class.poll_active_entity();
        class.flush_dirty_entities(false);

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
        for _ in 0..count {
            let report = self.state.next_status_report();
            self.push_report(report);
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
                }
            }
        }

        if persisted || !self.state.is_dirty() {
            self.dirty_write_deadline = None;
        }
    }

    fn push_report(&mut self, report: skylanders::Report) {
        for slot in &mut self.queue {
            if slot.is_none() {
                *slot = Some(report);
                return;
            }
        }
        println!("Skylanders USB response queue full; dropping oldest response");
        self.queue[REPORT_QUEUE_LEN - 1] = Some(report);
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

struct InfinityBaseClass<'a, B: usb_device::bus::UsbBus> {
    iface: InterfaceNumber,
    ep_in: EndpointIn<'a, B>,
    ep_out: EndpointOut<'a, B>,
    state: infinity::InfinityBaseState,
    queue: [Option<infinity::Report>; REPORT_QUEUE_LEN],
    out_buf: infinity::Report,
    idle_rate: u8,
    protocol: u8,
    storage_poll_ticks: u8,
    active_selection_marker: ([Option<u32>; infinity::MAX_FIGURES], u32),
    active_entity_ids: [Option<u32>; infinity::MAX_FIGURES],
    dirty_slots: [bool; infinity::MAX_FIGURES],
    dirty_write_deadline: Option<Instant>,
}

impl<'a, B: usb_device::bus::UsbBus> InfinityBaseClass<'a, B> {
    fn new(alloc: &'a UsbBusAllocator<B>) -> Self {
        Self {
            iface: alloc.interface(),
            ep_in: alloc
                .alloc(
                    Some(EndpointAddress::from(infinity::INTERRUPT_IN_ENDPOINT)),
                    EndpointType::Interrupt,
                    infinity::INTERRUPT_MAX_PACKET_SIZE,
                    infinity::INTERRUPT_POLL_INTERVAL_MS,
                )
                .expect("alloc Disney Infinity interrupt IN endpoint failed"),
            ep_out: alloc
                .alloc(
                    Some(EndpointAddress::from(infinity::INTERRUPT_OUT_ENDPOINT)),
                    EndpointType::Interrupt,
                    infinity::INTERRUPT_MAX_PACKET_SIZE,
                    infinity::INTERRUPT_POLL_INTERVAL_MS,
                )
                .expect("alloc Disney Infinity interrupt OUT endpoint failed"),
            state: infinity::InfinityBaseState::new(),
            queue: [None; REPORT_QUEUE_LEN],
            out_buf: [0; infinity::REPORT_BYTES],
            idle_rate: 0,
            protocol: 1,
            storage_poll_ticks: 0,
            active_selection_marker: ([None; infinity::MAX_FIGURES], 0),
            active_entity_ids: [None; infinity::MAX_FIGURES],
            dirty_slots: [false; infinity::MAX_FIGURES],
            dirty_write_deadline: None,
        }
    }

    fn poll_usb(&mut self) {
        self.poll_out_endpoint();
        self.poll_in_endpoint();
    }

    fn poll_out_endpoint(&mut self) {
        match self.ep_out.read(&mut self.out_buf) {
            Ok(infinity::REPORT_BYTES) => {
                let report = self.out_buf;
                self.handle_report(&report);
            }
            Ok(count) => {
                println!(
                    "Disney Infinity USB ignored short interrupt OUT report: len={}",
                    count
                );
            }
            Err(UsbError::WouldBlock) => {}
            Err(error) => {
                println!("Disney Infinity USB interrupt OUT read error: {:?}", error);
            }
        }
    }

    fn poll_in_endpoint(&mut self) {
        if let Some(report) = self.pop_report() {
            match self.ep_in.write(&report) {
                Ok(_) => {
                    println!(
                        "Disney Infinity USB sent report: {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x}",
                        report[0],
                        report[1],
                        report[2],
                        report[3],
                        report[4],
                        report[5],
                        report[6]
                    );
                }
                Err(UsbError::WouldBlock) => {
                    self.push_report_front(report);
                }
                Err(error) => {
                    println!(
                        "Disney Infinity USB interrupt IN queued write error: {:?}",
                        error
                    );
                }
            }
        }
    }

    fn handle_report(&mut self, report: &infinity::Report) {
        match report[0] {
            0x00 => {}
            0xaa | 0xab => {
                self.queue_pending_change_reports();
            }
            0xff => {
                let command = report[2];
                let order_added = report[4];
                if let Some(response) = infinity::handle_command_packet(&mut self.state, report) {
                    println!(
                        "Disney Infinity USB cmd=0x{:02x} seq=0x{:02x} rsp={:02x} {:02x} {:02x} {:02x}",
                        command,
                        report[3],
                        response.response[0],
                        response.response[1],
                        response.response[2],
                        response.response[3]
                    );
                    self.push_report(response.echo);
                    self.push_report(response.response);
                    if command == 0xa3 {
                        self.mark_dirty_order(order_added);
                    }
                } else {
                    println!("Disney Infinity USB rejected command packet");
                }
            }
            _ => {
                println!(
                    "Disney Infinity USB unhandled interrupt OUT report: op=0x{:02x}",
                    report[0]
                );
            }
        }
    }

    fn poll_active_entity(&mut self) {
        if self.storage_poll_ticks > 0 {
            self.storage_poll_ticks -= 1;
            return;
        }
        self.storage_poll_ticks = STORAGE_POLL_TICKS;

        let (active_slots, active_generation) = storage::active_slots_marker();
        let mut active_ids = [None; infinity::MAX_FIGURES];
        for (index, active_id) in active_ids.iter_mut().enumerate() {
            *active_id = active_slots[index].map(|id| id.0);
        }
        let active_marker = (active_ids, active_generation);
        if active_marker == self.active_selection_marker {
            return;
        }
        self.flush_dirty_entities(true);

        match storage::active_slot_images_for_game(GameLine::Infinity, infinity::MAX_FIGURES) {
            Ok(images) => {
                for slot in 0..infinity::MAX_FIGURES {
                    if active_marker.0[slot].is_none() && self.active_entity_ids[slot].is_some() {
                        if let Some(position) =
                            infinity::FigurePosition::from_portal_index(slot as u8)
                        {
                            println!(
                                "Disney Infinity USB removing position {} entity {}",
                                slot,
                                self.active_entity_ids[slot].unwrap_or(0)
                            );
                            self.state.remove_figure(position);
                            self.queue_pending_change_reports();
                        }
                        self.active_entity_ids[slot] = None;
                        self.dirty_slots[slot] = false;
                    }
                }

                let mut loaded = 0;
                for (slot, id, image) in images {
                    let Some(position) = infinity::FigurePosition::from_portal_index(slot) else {
                        continue;
                    };
                    if self.active_entity_ids[slot as usize] == Some(id.0) {
                        continue;
                    }
                    if self.state.load_figure(position, &image) {
                        self.active_entity_ids[slot as usize] = Some(id.0);
                        self.dirty_slots[slot as usize] = false;
                        loaded += 1;
                        println!(
                            "Disney Infinity USB loaded position {} entity {} ({} bytes)",
                            slot,
                            id.0,
                            image.len()
                        );
                        self.queue_pending_change_reports();
                    } else {
                        println!(
                            "Disney Infinity USB rejected slot {} entity {} image length {}",
                            slot,
                            id.0,
                            image.len()
                        );
                        self.state.remove_figure(position);
                        self.queue_pending_change_reports();
                        self.active_entity_ids[slot as usize] = None;
                        self.dirty_slots[slot as usize] = false;
                    }
                }

                self.active_selection_marker = active_marker;
                if loaded > 0 {
                    println!(
                        "Disney Infinity USB loaded {} active portal position(s)",
                        loaded
                    );
                }
            }
            Err(error) => {
                println!(
                    "Disney Infinity USB failed to read active placement state: {} ({:?})",
                    error.message(),
                    error
                );
                self.clear_all_positions();
                self.active_selection_marker = ([None; infinity::MAX_FIGURES], 0);
            }
        }
    }

    fn mark_dirty_order(&mut self, order_added: u8) {
        let Some(position) = self.state.figure_position_by_order(order_added) else {
            return;
        };
        let index = position.portal_index() as usize;
        if self.active_entity_ids[index].is_none() {
            return;
        }
        self.dirty_slots[index] = true;
        self.dirty_write_deadline = Some(Instant::now() + STORAGE_WRITE_DEBOUNCE);
    }

    fn flush_dirty_entities(&mut self, force: bool) {
        if !self.dirty_slots.iter().any(|dirty| *dirty) {
            self.dirty_write_deadline = None;
            return;
        }

        if !force {
            match self.dirty_write_deadline {
                Some(deadline) if Instant::now() < deadline => return,
                Some(_) => {}
                None => {
                    self.dirty_write_deadline = Some(Instant::now() + STORAGE_WRITE_DEBOUNCE);
                    return;
                }
            }
        }

        let mut still_dirty = false;
        for slot in 0..infinity::MAX_FIGURES {
            if !self.dirty_slots[slot] {
                continue;
            }
            let Some(entity_id) = self.active_entity_ids[slot] else {
                self.dirty_slots[slot] = false;
                continue;
            };
            let Some(position) = infinity::FigurePosition::from_portal_index(slot as u8) else {
                self.dirty_slots[slot] = false;
                continue;
            };
            let Some(image) = self.state.figure_image(position).copied() else {
                self.dirty_slots[slot] = false;
                continue;
            };
            match storage::replace_entity_blob(RecordId(entity_id), &image) {
                Ok(()) => {
                    println!(
                        "Disney Infinity USB persisted writes for position {} entity {}",
                        slot, entity_id
                    );
                    self.dirty_slots[slot] = false;
                }
                Err(error) => {
                    println!(
                        "Disney Infinity USB failed to persist writes for position {} entity {}: {:?}",
                        slot, entity_id, error
                    );
                    still_dirty = true;
                }
            }
        }

        self.dirty_write_deadline = if still_dirty {
            Some(Instant::now() + STORAGE_WRITE_DEBOUNCE)
        } else {
            None
        };
    }

    fn clear_all_positions(&mut self) {
        for slot in 0..infinity::MAX_FIGURES {
            if let Some(position) = infinity::FigurePosition::from_portal_index(slot as u8) {
                self.state.remove_figure(position);
                self.queue_pending_change_reports();
            }
            self.active_entity_ids[slot] = None;
            self.dirty_slots[slot] = false;
        }
    }

    fn push_report(&mut self, report: infinity::Report) {
        for slot in &mut self.queue {
            if slot.is_none() {
                *slot = Some(report);
                return;
            }
        }
        println!("Disney Infinity USB response queue full; dropping oldest response");
        self.queue[REPORT_QUEUE_LEN - 1] = Some(report);
    }

    fn queue_pending_change_reports(&mut self) {
        while let Some(change) = self.state.pop_change_response() {
            println!(
                "Disney Infinity USB queueing change report: {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x}",
                change[0],
                change[1],
                change[2],
                change[3],
                change[4],
                change[5],
                change[6]
            );
            for _ in 0..INFINITY_CHANGE_REPORT_REPEATS {
                self.push_report_front(change);
            }
        }
    }

    fn push_report_front(&mut self, report: infinity::Report) {
        for index in (1..REPORT_QUEUE_LEN).rev() {
            self.queue[index] = self.queue[index - 1];
        }
        self.queue[0] = Some(report);
    }

    fn pop_report(&mut self) -> Option<infinity::Report> {
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

impl<B: usb_device::bus::UsbBus> UsbClass<B> for InfinityBaseClass<'_, B> {
    fn get_configuration_descriptors(
        &self,
        writer: &mut DescriptorWriter,
    ) -> usb_device::Result<()> {
        writer.interface(self.iface, infinity::HID_CLASS, 0x00, 0x00)?;
        writer.write(
            infinity::HID_DESCRIPTOR_TYPE,
            &[
                0x11,
                0x01,
                0x00,
                0x01,
                infinity::HID_REPORT_DESCRIPTOR_TYPE,
                infinity::HID_REPORT_DESCRIPTOR.len() as u8,
                (infinity::HID_REPORT_DESCRIPTOR.len() >> 8) as u8,
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
            && req.descriptor_type_index().0 == infinity::HID_REPORT_DESCRIPTOR_TYPE
        {
            let _ = xfer.accept_with_static(infinity::HID_REPORT_DESCRIPTOR);
            return;
        }

        if req.request_type == RequestType::Class && req.recipient == Recipient::Interface {
            match req.request {
                infinity::HID_GET_REPORT_REQUEST => {
                    let report = self
                        .pop_report()
                        .unwrap_or_else(|| self.state.present_figures_response(self.out_buf[3]));
                    let _ = xfer.accept_with(&report);
                }
                infinity::HID_GET_IDLE_REQUEST => {
                    let _ = xfer.accept_with(&[self.idle_rate]);
                }
                infinity::HID_GET_PROTOCOL_REQUEST => {
                    let _ = xfer.accept_with(&[self.protocol]);
                }
                _ => {
                    println!(
                        "Disney Infinity USB unhandled control IN request: type={:?}, recipient={:?}, request=0x{:02x}, value=0x{:04x}, index=0x{:04x}, len={}",
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
                infinity::HID_SET_REPORT_REQUEST => {
                    let data = xfer.data();
                    if data.len() == infinity::REPORT_BYTES {
                        let mut report = [0; infinity::REPORT_BYTES];
                        report.copy_from_slice(data);
                        self.handle_report(&report);
                    }
                    let _ = xfer.accept();
                }
                infinity::HID_SET_IDLE_REQUEST => {
                    self.idle_rate = (req.value >> 8) as u8;
                    let _ = xfer.accept();
                }
                infinity::HID_SET_PROTOCOL_REQUEST => {
                    self.protocol = req.value as u8;
                    let _ = xfer.accept();
                }
                _ => {
                    println!(
                        "Disney Infinity USB unhandled control OUT request: type={:?}, recipient={:?}, request=0x{:02x}, value=0x{:04x}, index=0x{:04x}, len={}",
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
