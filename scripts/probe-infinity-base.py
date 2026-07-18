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


def poll_until_response(ep_out, ep_in, expected_prefix: bytes) -> bytes:
    last = b""
    for _ in range(8):
        response = poll_response(ep_out, ep_in)
        if response.startswith(expected_prefix):
            return response
        if response.startswith(b"\xAB"):
            print(f"Change report: {response[:7].hex(' ')}")
        last = response
    raise RuntimeError(
        f"Did not receive {expected_prefix.hex(' ')} response; last was {last.hex(' ')}"
    )


def command_response(
    ep_out, ep_in, command: int, sequence: int, payload: bytes = b""
) -> bytes:
    request = command_packet(command, sequence, payload)
    write_report(ep_out, request)
    echo = read_report(ep_in)
    if echo != request:
        raise RuntimeError(
            f"Unexpected command echo for 0x{command:02x}: {echo.hex(' ')}"
        )
    return poll_until_response(ep_out, ep_in, bytes([0xAA]))


def verify_response_checksum(label: str, report: bytes) -> None:
    length = report[1]
    checksum_index = 2 + length
    if checksum_index >= len(report):
        raise RuntimeError(f"{label} checksum index out of range: {report.hex(' ')}")
    actual = report[checksum_index]
    expected = checksum(report[:checksum_index])
    if actual != expected:
        raise RuntimeError(
            f"{label} checksum mismatch: got 0x{actual:02x}, expected 0x{expected:02x}"
        )


def first_present_order(presence: bytes):
    if presence[1] < 3:
        return None
    position_order = presence[3]
    return position_order & 0x0F


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

        activate_response = poll_until_response(ep_out, ep_in, bytes([0xAA, 0x15]))
        if not activate_response.startswith(bytes.fromhex("aa 15 00 00 0f 01")):
            print(
                f"Unexpected activate response: {activate_response.hex(' ')}",
                file=sys.stderr,
            )
            return 1

        presence_response = command_response(ep_out, ep_in, 0xA1, 0x02)
        if presence_response[:4] != bytes([0xAA, 0x01, 0x02, 0xAD]):
            verify_response_checksum("presence response", presence_response)

        print("Disney Infinity USB base probe OK")
        print(f"Activate response: {activate_response[:24].hex(' ')}")
        print(f"Presence response: {presence_response[:12].hex(' ')}")

        order_added = first_present_order(presence_response)
        if order_added is not None:
            tag_response = command_response(
                ep_out, ep_in, 0xB4, 0x03, bytes([order_added])
            )
            verify_response_checksum("tag response", tag_response)
            block_response = command_response(
                ep_out, ep_in, 0xA2, 0x04, bytes([order_added, 0x00, 0x00])
            )
            verify_response_checksum("block response", block_response)
            print(f"First figure order: {order_added}")
            print(f"Tag ID: {tag_response[4:11].hex(' ')}")
            print(f"Block 1: {block_response[4:20].hex(' ')}")
        return 0
    except (RuntimeError, usb.core.USBError) as error:
        print(f"USB probe failed: {error}", file=sys.stderr)
        return 1
    finally:
        usb.util.dispose_resources(dev)


if __name__ == "__main__":
    raise SystemExit(main())
