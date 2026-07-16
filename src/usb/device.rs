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

const REPORT_QUEUE_LEN: usize = 4;
const STORAGE_POLL_TICKS: u8 = 50;
#[embassy_executor::task]
pub async fn run(usb0: USB0<'static>, usb_dp: GPIO20<'static>, usb_dm: GPIO19<'static>) {
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
        if usb_dev.poll(&mut [&mut class]) {
            class.poll();
        }

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
        }
    }

    fn poll(&mut self) {
        self.poll_active_entity();
        self.poll_out_endpoint();
        self.poll_in_endpoint();
        self.flush_dirty_entity();
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

        let report = self.state.next_status_report();
        match self.ep_in.write(&report) {
            Ok(_) | Err(UsbError::WouldBlock) => {}
            Err(error) => {
                println!(
                    "Skylanders USB interrupt IN status write error: {:?}",
                    error
                );
            }
        }
    }

    fn handle_command_source(&mut self, source: &str, command: &[u8]) {
        let should_log = should_log_command(command);
        if should_log {
            log_command(source, command);
        }
        if let Some(response) = skylanders::handle_command(&mut self.state, command) {
            if should_log || matches!(response.report[0], b'Q' | b'W') {
                log_response(response.queue_report, &response.report);
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

        let active_id = storage::active_entity_id().map(|id| id.0);
        if active_id == self.state.active_entity_id() {
            return;
        }

        self.flush_dirty_entity();
        match storage::active_entity_image() {
            Ok(Some((id, image))) => {
                if self.state.load_entity(id.0, &image) {
                    println!(
                        "Skylanders USB loaded active entity {} ({} bytes)",
                        id.0,
                        image.len()
                    );
                } else {
                    println!(
                        "Skylanders USB rejected active entity {} image length {}",
                        id.0,
                        image.len()
                    );
                    self.state.clear_entity();
                }
            }
            Ok(None) => {
                println!("Skylanders USB active entity cleared");
                self.state.clear_entity();
            }
            Err(error) => {
                println!("Skylanders USB failed to read active entity: {:?}", error);
                self.state.clear_entity();
            }
        }
    }

    fn flush_dirty_entity(&mut self) {
        if !self.state.is_dirty() {
            return;
        }

        if let Some(id) = self.state.active_entity_id() {
            match storage::replace_entity_blob(RecordId(id), self.state.image()) {
                Ok(()) => {
                    println!("Skylanders USB persisted writes for entity {}", id);
                    self.state.clear_dirty();
                }
                Err(error) => {
                    println!(
                        "Skylanders USB failed to persist writes for entity {}: {:?}",
                        id, error
                    );
                }
            }
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

fn should_log_command(command: &[u8]) -> bool {
    matches!(
        command.first().copied().unwrap_or(0),
        b'Q' | b'W' | b'M' | b'J' | b'L' | b'V' | b'Z'
    )
}

fn log_command(source: &str, command: &[u8]) {
    println!(
        "Skylanders USB cmd {}: len={}, bytes={:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x}",
        source,
        command.len(),
        byte(command, 0),
        byte(command, 1),
        byte(command, 2),
        byte(command, 3),
        byte(command, 4),
        byte(command, 5),
        byte(command, 6),
        byte(command, 7)
    );
}

fn log_response(queued: bool, report: &[u8; skylanders::REPORT_BYTES]) {
    println!(
        "Skylanders USB rsp queued={}: bytes={:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x} {:02x}",
        queued,
        report[0],
        report[1],
        report[2],
        report[3],
        report[4],
        report[5],
        report[6],
        report[7]
    );
}

fn byte(bytes: &[u8], index: usize) -> u8 {
    bytes.get(index).copied().unwrap_or(0)
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
            println!("Skylanders USB GET_DESCRIPTOR report");
            let _ = xfer.accept_with_static(skylanders::HID_REPORT_DESCRIPTOR);
            return;
        }

        if req.request_type == RequestType::Class && req.recipient == Recipient::Interface {
            match req.request {
                skylanders::HID_GET_REPORT_REQUEST => {
                    let report = self.state.next_status_report();
                    println!("Skylanders USB GET_REPORT status");
                    let _ = xfer.accept_with(&report);
                }
                skylanders::HID_GET_IDLE_REQUEST => {
                    println!("Skylanders USB GET_IDLE");
                    let _ = xfer.accept_with(&[self.idle_rate]);
                }
                skylanders::HID_GET_PROTOCOL_REQUEST => {
                    println!("Skylanders USB GET_PROTOCOL protocol={}", self.protocol);
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
                    println!("Skylanders USB SET_IDLE rate={}", self.idle_rate);
                    let _ = xfer.accept();
                }
                skylanders::HID_SET_PROTOCOL_REQUEST => {
                    self.protocol = req.value as u8;
                    println!("Skylanders USB SET_PROTOCOL protocol={}", self.protocol);
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
