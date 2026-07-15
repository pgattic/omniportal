use embassy_time::{Duration, Timer};
use esp_hal::{
    otg_fs::{Usb, UsbBus as EspUsbBus},
    peripherals::{GPIO19, GPIO20, USB0},
};
use static_cell::StaticCell;
use usb_device::{
    class_prelude::*,
    control::{Recipient, Request, RequestType},
    prelude::{StringDescriptors, UsbDeviceBuilder, UsbVidPid},
};

use crate::usb::skylanders;

const REPORT_QUEUE_LEN: usize = 4;

#[embassy_executor::task]
pub async fn run(usb0: USB0<'static>, usb_dp: GPIO20<'static>, usb_dm: GPIO19<'static>) {
    static EP_MEMORY: StaticCell<[u32; 1024]> = StaticCell::new();

    let usb = Usb::new(usb0, usb_dp, usb_dm);
    let usb_bus = EspUsbBus::new(usb, EP_MEMORY.init([0; 1024]));
    let mut class = SkylandersPortalClass::new(&usb_bus);
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
}

impl<'a, B: usb_device::bus::UsbBus> SkylandersPortalClass<'a, B> {
    fn new(alloc: &'a UsbBusAllocator<B>) -> Self {
        Self {
            iface: alloc.interface(),
            ep_in: alloc.interrupt(
                skylanders::INTERRUPT_MAX_PACKET_SIZE,
                skylanders::INTERRUPT_POLL_INTERVAL_MS,
            ),
            ep_out: alloc.interrupt(
                skylanders::INTERRUPT_MAX_PACKET_SIZE,
                skylanders::INTERRUPT_POLL_INTERVAL_MS,
            ),
            state: skylanders::PortalState::new(),
            queue: [None; REPORT_QUEUE_LEN],
            out_buf: [0; skylanders::MAX_PACKET_BYTES],
            idle_rate: 0,
        }
    }

    fn poll(&mut self) {
        self.poll_out_endpoint();
        self.poll_in_endpoint();
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
                Err(UsbError::WouldBlock) => {
                    self.push_report_front(report);
                }
                Err(_) => {}
            }
            return;
        }

        let report = self.state.next_status_report();
        match self.ep_in.write(&report) {
            Ok(_) | Err(UsbError::WouldBlock) => {}
            Err(_) => {}
        }
    }

    fn handle_command(&mut self, command: &[u8]) {
        if let Some(response) = skylanders::handle_command(&mut self.state, command) {
            if response.queue_report {
                self.push_report(response.report);
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
                    let report = self.state.next_status_report();
                    let _ = xfer.accept_with(&report);
                }
                skylanders::HID_GET_IDLE_REQUEST => {
                    let _ = xfer.accept_with(&[self.idle_rate]);
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
                _ => {}
            }
        }
    }
}
