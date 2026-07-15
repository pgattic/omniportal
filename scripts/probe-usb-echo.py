#!/usr/bin/env python3
import sys

import usb.core
import usb.util

VID = 0xCAFE
PID = 0x4001
INTERFACE = 0
TIMEOUT_MS = 1000
PAYLOAD = b"omniportal usb echo"


def main() -> int:
    dev = usb.core.find(idVendor=VID, idProduct=PID)
    if dev is None:
        print("OmniPortal development USB device not found", file=sys.stderr)
        return 1

    try:
        if dev.is_kernel_driver_active(INTERFACE):
            dev.detach_kernel_driver(INTERFACE)
    except (NotImplementedError, usb.core.USBError):
        pass

    try:
        dev.set_configuration()
        cfg = dev.get_active_configuration()
        intf = cfg[(INTERFACE, 0)]

        ep_out = usb.util.find_descriptor(
            intf,
            custom_match=lambda ep: usb.util.endpoint_direction(ep.bEndpointAddress)
            == usb.util.ENDPOINT_OUT,
        )
        ep_in = usb.util.find_descriptor(
            intf,
            custom_match=lambda ep: usb.util.endpoint_direction(ep.bEndpointAddress)
            == usb.util.ENDPOINT_IN,
        )
        if ep_out is None or ep_in is None:
            print("Bulk echo endpoints not found", file=sys.stderr)
            return 1

        ep_out.write(PAYLOAD, TIMEOUT_MS)
        echoed = bytes(ep_in.read(64, TIMEOUT_MS))[: len(PAYLOAD)]
        if echoed != PAYLOAD:
            print(f"Unexpected echo: {echoed!r}", file=sys.stderr)
            return 1

        print("USB echo OK")
        return 0
    except usb.core.USBError as error:
        print(f"USB probe failed: {error}", file=sys.stderr)
        return 1
    finally:
        usb.util.dispose_resources(dev)


if __name__ == "__main__":
    raise SystemExit(main())
