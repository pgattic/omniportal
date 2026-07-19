#!/usr/bin/env python3
import argparse
import struct
from pathlib import Path

UF2_BLOCK_SIZE = 512
PAYLOAD_OFFSET = 32
UF2_MAGIC_START0 = 0x0A324655
UF2_MAGIC_START1 = 0x9E5D5157
UF2_MAGIC_END = 0x0AB16F30
RP2350_BLOCK_MARKER_START = 0xFFFFDED3


def main() -> int:
    parser = argparse.ArgumentParser(description="Print basic UF2 block metadata.")
    parser.add_argument("uf2", type=Path)
    args = parser.parse_args()

    data = args.uf2.read_bytes()
    if len(data) % UF2_BLOCK_SIZE:
        raise SystemExit(f"{args.uf2}: size is not a multiple of {UF2_BLOCK_SIZE}")

    for index, offset in enumerate(range(0, len(data), UF2_BLOCK_SIZE)):
        block = data[offset : offset + UF2_BLOCK_SIZE]
        magic0, magic1, flags, address, payload_size, block_no, block_count, family = (
            struct.unpack_from("<IIIIIIII", block, 0)
        )
        magic_end = struct.unpack_from("<I", block, 508)[0]
        if (
            magic0 != UF2_MAGIC_START0
            or magic1 != UF2_MAGIC_START1
            or magic_end != UF2_MAGIC_END
        ):
            raise SystemExit(f"{args.uf2}: invalid UF2 block {index}")

        payload = block[PAYLOAD_OFFSET : PAYLOAD_OFFSET + payload_size]
        marker_offset = payload.find(struct.pack("<I", RP2350_BLOCK_MARKER_START))
        marker_text = ""
        if marker_offset >= 0:
            marker_text = f" rp2350-block-marker=0x{address + marker_offset:08x}"

        if index < 4 or marker_offset >= 0:
            print(
                f"block={block_no}/{block_count} address=0x{address:08x} "
                f"size={payload_size} family=0x{family:08x} flags=0x{flags:08x}"
                f"{marker_text}"
            )

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
