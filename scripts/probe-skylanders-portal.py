#!/usr/bin/env python3
import argparse
import sys
import time

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

SLOT_STATUS = {
    0: "removed",
    1: "ready",
    2: "removing",
    3: "added",
}


def find_interrupt_in(interface):
    return usb.util.find_descriptor(
        interface,
        custom_match=lambda ep: usb.util.endpoint_direction(ep.bEndpointAddress)
        == usb.util.ENDPOINT_IN,
    )


def find_interrupt_out(interface):
    return usb.util.find_descriptor(
        interface,
        custom_match=lambda ep: usb.util.endpoint_direction(ep.bEndpointAddress)
        == usb.util.ENDPOINT_OUT,
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


def get_status_report(dev) -> bytes:
    last = b""
    for _ in range(8):
        set_report(dev, b"S")
        packet = get_report(dev)
        if packet.startswith(b"S"):
            return packet
        last = packet
    raise RuntimeError(f"Did not receive status report; last packet was {last!r}")


def decode_slot_statuses(status: bytes) -> list[str]:
    packed = int.from_bytes(status[1:5], "little")
    return [SLOT_STATUS[(packed >> (slot * 2)) & 0x03] for slot in range(16)]


def watch_status(dev, ep_in, seconds: float) -> None:
    deadline = time.monotonic() + seconds
    last_slots = None
    while time.monotonic() < deadline:
        try:
            status = get_status_report(dev)
        except usb.core.USBError:
            continue
        if not status.startswith(b"S"):
            continue
        slots = decode_slot_statuses(status)
        if slots != last_slots:
            print(f"{time.monotonic():.3f}: {status[:7].hex(' ')} slot0={slots[0]}")
            last_slots = slots
        time.sleep(0.05)


def watch_status_with_repeated_activate(dev, ep_in, seconds: float) -> None:
    deadline = time.monotonic() + seconds
    last_slots = None
    while time.monotonic() < deadline:
        try:
            set_report(dev, b"A\x01")
            status = get_status_report(dev)
        except usb.core.USBError:
            continue
        if not status.startswith(b"S"):
            continue
        slots = decode_slot_statuses(status)
        if slots != last_slots:
            print(f"{time.monotonic():.3f}: {status[:7].hex(' ')} slot0={slots[0]}")
            last_slots = slots
        time.sleep(0.05)


def wait_for_slot_ready(dev, seconds: float) -> bytes:
    deadline = time.monotonic() + seconds
    last = b""
    while time.monotonic() < deadline:
        status = get_status_report(dev)
        last = status
        if status.startswith(b"S") and decode_slot_statuses(status)[0] == "ready":
            return status
        time.sleep(0.05)
    raise RuntimeError(f"slot0 did not become ready; last status was {last[:7].hex(' ')}")


def read_block(dev, ep_in, slot_id: int, block: int) -> bytes:
    set_report(dev, bytes([ord("Q"), slot_id, block]))
    response = read_until(ep_in, b"Q")
    if len(response) < 3 or response[1] != slot_id or response[2] != block:
        raise RuntimeError(
            f"unexpected block {block} response: {response[:19].hex(' ')}"
        )
    return response[3:19]


def read_block_interrupt(ep_out, ep_in, slot_id: int, block: int) -> bytes:
    ep_out.write(bytes([ord("Q"), slot_id, block]).ljust(REPORT_BYTES, b"\x00"), TIMEOUT_MS)
    response = read_until(ep_in, b"Q")
    if len(response) < 3 or response[1] != slot_id or response[2] != block:
        raise RuntimeError(
            f"unexpected interrupt block {block} response: {response[:19].hex(' ')}"
        )
    return response[3:19]


def read_blocks(dev, ep_in, blocks: list[int], ep_out=None) -> None:
    ready = wait_for_slot_ready(dev, 10)
    print(f"Ready status: {ready[:7].hex(' ')}")
    for block in blocks:
        if ep_out is None:
            data = read_block(dev, ep_in, 0x10, block)
        else:
            data = read_block_interrupt(ep_out, ep_in, 0x10, block)
        print(f"Block {block:02x}: {data.hex(' ')}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--watch-status",
        type=float,
        metavar="SECONDS",
        help="poll status reports after activation and print slot status changes",
    )
    parser.add_argument(
        "--watch-repeated-activate",
        type=float,
        metavar="SECONDS",
        help="poll status while repeatedly sending A 01, matching Wii-style waiting behavior",
    )
    parser.add_argument(
        "--read-blocks",
        nargs="*",
        type=lambda value: int(value, 0),
        metavar="BLOCK",
        help="wait for slot0 readiness and read figure blocks, defaulting to 0, 1, and 2",
    )
    parser.add_argument(
        "--read-blocks-interrupt-out",
        nargs="*",
        type=lambda value: int(value, 0),
        metavar="BLOCK",
        help="read figure blocks by sending Q commands through interrupt OUT",
    )
    args = parser.parse_args()

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
        ep_out = find_interrupt_out(interface)
        if args.read_blocks_interrupt_out is not None and ep_out is None:
            print("Interrupt OUT endpoint not found", file=sys.stderr)
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
        if args.watch_status:
            watch_status(dev, ep_in, args.watch_status)
        if args.watch_repeated_activate:
            watch_status_with_repeated_activate(dev, ep_in, args.watch_repeated_activate)
        if args.read_blocks is not None:
            read_blocks(dev, ep_in, args.read_blocks or [0, 1, 2])
        if args.read_blocks_interrupt_out is not None:
            read_blocks(dev, ep_in, args.read_blocks_interrupt_out or [0, 1, 2], ep_out)
        return 0
    except usb.core.USBError as error:
        print(f"USB probe failed: {error}", file=sys.stderr)
        return 1
    finally:
        usb.util.dispose_resources(dev)


if __name__ == "__main__":
    raise SystemExit(main())
