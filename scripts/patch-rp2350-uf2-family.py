#!/usr/bin/env python3
import argparse
import struct
import sys
from pathlib import Path

UF2_BLOCK_SIZE = 512
UF2_MAGIC_START0 = 0x0A324655
UF2_MAGIC_START1 = 0x9E5D5157
UF2_MAGIC_END = 0x0AB16F30
UF2_FLAG_FAMILY_ID_PRESENT = 0x00002000

RP2350_ARM_S_FAMILY_ID = 0xE48BFF59


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Patch an elf2uf2-rs UF2 image for RP2350 ARM secure bootroms."
    )
    parser.add_argument("uf2", type=Path)
    args = parser.parse_args()

    data = bytearray(args.uf2.read_bytes())
    if len(data) == 0 or len(data) % UF2_BLOCK_SIZE != 0:
        print(f"{args.uf2}: invalid UF2 size {len(data)}", file=sys.stderr)
        return 1

    for offset in range(0, len(data), UF2_BLOCK_SIZE):
        block = data[offset : offset + UF2_BLOCK_SIZE]
        magic0, magic1, flags = struct.unpack_from("<III", block, 0)
        magic_end = struct.unpack_from("<I", block, 508)[0]
        if (
            magic0 != UF2_MAGIC_START0
            or magic1 != UF2_MAGIC_START1
            or magic_end != UF2_MAGIC_END
        ):
            print(f"{args.uf2}: invalid UF2 block at offset {offset}", file=sys.stderr)
            return 1

        flags |= UF2_FLAG_FAMILY_ID_PRESENT
        struct.pack_into("<I", data, offset + 8, flags)
        struct.pack_into("<I", data, offset + 28, RP2350_ARM_S_FAMILY_ID)

    args.uf2.write_bytes(data)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
