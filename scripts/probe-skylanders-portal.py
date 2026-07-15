#!/usr/bin/env python3
import sys

import usb.core
import usb.util

VID = 0x1430
PID = 0x0150
INTERFACE = 0
TIMEOUT_MS = 1000
REPORT_BYTES = 32

HID_GET_REPORT = 0x01
HID_SET_REPORT = 0x09
REPORT_TYPE_INPUT = 0x01
REPORT_TYPE_FEATURE = 0x03


def find_interrupt_in(interface):
    return usb.util.find_descriptor(
        interface,
        custom_match=lambda ep: usb.util.endpoint_direction(ep.bEndpointAddress)
        == usb.util.ENDPOINT_IN,
    )


def set_report(dev, command: bytes) -> None:
    report = command.ljust(REPORT_BYTES, b"\x00")
    dev.ctrl_transfer(
        0x21,
        HID_SET_REPORT,
        REPORT_TYPE_FEATURE << 8,
        INTERFACE,
        report,
        TIMEOUT_MS,
    )


def get_report(dev) -> bytes:
    return bytes(
        dev.ctrl_transfer(
            0xA1,
            HID_GET_REPORT,
            REPORT_TYPE_INPUT << 8,
            INTERFACE,
            REPORT_BYTES,
            TIMEOUT_MS,
        )
    )


def read_until(ep_in, expected_prefix: bytes) -> bytes:
    last = b""
    for _ in range(8):
        packet = bytes(ep_in.read(REPORT_BYTES, TIMEOUT_MS))
        if packet.startswith(expected_prefix):
            return packet
        last = packet
    raise RuntimeError(f"Did not receive {expected_prefix!r}; last packet was {last!r}")


def main() -> int:
    dev = usb.core.find(idVendor=VID, idProduct=PID)
    if dev is None:
        print("Skylanders Portal of Power USB device not found", file=sys.stderr)
        return 1

    try:
        if dev.is_kernel_driver_active(INTERFACE):
            dev.detach_kernel_driver(INTERFACE)
    except (NotImplementedError, usb.core.USBError):
        pass

    try:
        dev.set_configuration()
        cfg = dev.get_active_configuration()
        interface = cfg[(INTERFACE, 0)]
        ep_in = find_interrupt_in(interface)
        if ep_in is None:
            print("Interrupt IN endpoint not found", file=sys.stderr)
            return 1

        set_report(dev, b"A\x01")
        queued = read_until(ep_in, b"A\x01\xff\x77")

        status = get_report(dev)
        if not status.startswith(b"S"):
            print(f"Unexpected status report: {status!r}", file=sys.stderr)
            return 1

        print("Skylanders portal USB probe OK")
        print(f"Activate response: {queued[:4].hex(' ')}")
        print(f"Status report: {status[:7].hex(' ')}")
        return 0
    except usb.core.USBError as error:
        print(f"USB probe failed: {error}", file=sys.stderr)
        return 1
    finally:
        usb.util.dispose_resources(dev)


if __name__ == "__main__":
    raise SystemExit(main())
