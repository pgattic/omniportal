use embassy_time::{Duration, Timer};
use esp_hal::{
    otg_fs::{Usb, UsbBus as EspUsbBus},
    peripherals::{GPIO19, GPIO20, USB0},
};
use static_cell::StaticCell;
use usb_device::{
    class_prelude::*,
    prelude::{StringDescriptors, UsbDeviceBuilder, UsbVidPid},
};

const DEV_VID: u16 = 0xcafe;
const DEV_PID: u16 = 0x4001;
const MAX_PACKET_SIZE: u16 = 64;

#[embassy_executor::task]
pub async fn run(usb0: USB0<'static>, usb_dp: GPIO20<'static>, usb_dm: GPIO19<'static>) {
    static EP_MEMORY: StaticCell<[u32; 1024]> = StaticCell::new();

    let usb = Usb::new(usb0, usb_dp, usb_dm);
    let usb_bus = EspUsbBus::new(usb, EP_MEMORY.init([0; 1024]));
    let mut class = EchoClass::new(&usb_bus);
    let mut usb_dev = UsbDeviceBuilder::new(&usb_bus, UsbVidPid(DEV_VID, DEV_PID))
        .strings(&[StringDescriptors::default()
            .manufacturer("OmniPortal")
            .product("OmniPortal development USB")
            .serial_number("0001")])
        .unwrap()
        .max_packet_size_0(64)
        .unwrap()
        .device_class(0xff)
        .build();

    loop {
        if usb_dev.poll(&mut [&mut class]) {
            class.poll_endpoints();
        }

        Timer::after(Duration::from_millis(1)).await;
    }
}

struct EchoClass<'a, B: usb_device::bus::UsbBus> {
    iface: InterfaceNumber,
    ep_in: EndpointIn<'a, B>,
    ep_out: EndpointOut<'a, B>,
    pending: [u8; MAX_PACKET_SIZE as usize],
    pending_len: usize,
}

impl<'a, B: usb_device::bus::UsbBus> EchoClass<'a, B> {
    fn new(alloc: &'a UsbBusAllocator<B>) -> Self {
        Self {
            iface: alloc.interface(),
            ep_in: alloc.bulk(MAX_PACKET_SIZE),
            ep_out: alloc.bulk(MAX_PACKET_SIZE),
            pending: [0; MAX_PACKET_SIZE as usize],
            pending_len: 0,
        }
    }

    fn poll_endpoints(&mut self) {
        if self.pending_len == 0 {
            match self.ep_out.read(&mut self.pending) {
                Ok(count) => {
                    self.pending_len = count;
                }
                Err(UsbError::WouldBlock) => {}
                Err(_) => {
                    self.pending_len = 0;
                }
            }
        }

        if self.pending_len > 0 {
            match self.ep_in.write(&self.pending[..self.pending_len]) {
                Ok(_) => {
                    self.pending_len = 0;
                }
                Err(UsbError::WouldBlock) => {}
                Err(_) => {
                    self.pending_len = 0;
                }
            }
        }
    }
}

impl<B: usb_device::bus::UsbBus> UsbClass<B> for EchoClass<'_, B> {
    fn get_configuration_descriptors(
        &self,
        writer: &mut DescriptorWriter,
    ) -> usb_device::Result<()> {
        writer.interface(self.iface, 0xff, 0x00, 0x00)?;
        writer.endpoint(&self.ep_in)?;
        writer.endpoint(&self.ep_out)?;
        Ok(())
    }
}
