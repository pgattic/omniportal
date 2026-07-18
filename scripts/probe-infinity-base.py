#!/usr/bin/env python3
import sys

import usb.core
import usb.util

VID = 0x0E6F
PID = 0x0129
INTERFACE = 0
TIMEOUT_MS = 1000
REPORT_BYTES = 32


def find_endpoint(interface, direction):
    return usb.util.find_descriptor(
        interface,
        custom_match=lambda ep: usb.util.endpoint_direction(ep.bEndpointAddress) == direction,
    )


def checksum(report: bytes) -> int:
    return sum(report) & 0xFF


def command_packet(command: int, sequence: int, payload: bytes = b"") -> bytes:
    length = len(payload) + 2
    packet = bytearray(REPORT_BYTES)
    packet[0] = 0xFF
    packet[1] = length
    packet[2] = command
    packet[3] = sequence
    packet[4 : 4 + len(payload)] = payload
    packet[2 + length] = checksum(packet[: 2 + length])
    return bytes(packet)


def write_report(ep_out, report: bytes) -> None:
    ep_out.write(report.ljust(REPORT_BYTES, b"\x00"), TIMEOUT_MS)


def read_report(ep_in) -> bytes:
    return bytes(ep_in.read(REPORT_BYTES, TIMEOUT_MS))


def poll_response(ep_out, ep_in) -> bytes:
    write_report(ep_out, bytes([0xAA]))
    return read_report(ep_in)


def main() -> int:
    dev = usb.core.find(idVendor=VID, idProduct=PID)
    if dev is None:
        print("Disney Infinity USB base not found", file=sys.stderr)
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
        ep_in = find_endpoint(interface, usb.util.ENDPOINT_IN)
        ep_out = find_endpoint(interface, usb.util.ENDPOINT_OUT)
        if ep_in is None or ep_out is None:
            print("Interrupt endpoints not found", file=sys.stderr)
            return 1

        activate = command_packet(0x80, 0x01)
        write_report(ep_out, activate)
        echo = read_report(ep_in)
        if echo != activate:
            print(f"Unexpected activate echo: {echo.hex(' ')}", file=sys.stderr)
            return 1

        activate_response = poll_response(ep_out, ep_in)
        if not activate_response.startswith(bytes.fromhex("aa 15 00 00 0f 01")):
            print(
                f"Unexpected activate response: {activate_response.hex(' ')}",
                file=sys.stderr,
            )
            return 1

        presence = command_packet(0xA1, 0x02)
        write_report(ep_out, presence)
        echo = read_report(ep_in)
        if echo != presence:
            print(f"Unexpected presence echo: {echo.hex(' ')}", file=sys.stderr)
            return 1

        presence_response = poll_response(ep_out, ep_in)
        if presence_response[:4] != bytes([0xAA, 0x01, 0x02, 0xAD]):
            print(
                f"Unexpected empty presence response: {presence_response.hex(' ')}",
                file=sys.stderr,
            )
            return 1

        print("Disney Infinity USB base probe OK")
        print(f"Activate response: {activate_response[:24].hex(' ')}")
        print(f"Empty presence: {presence_response[:4].hex(' ')}")
        return 0
    except usb.core.USBError as error:
        print(f"USB probe failed: {error}", file=sys.stderr)
        return 1
    finally:
        usb.util.dispose_resources(dev)


if __name__ == "__main__":
    raise SystemExit(main())
