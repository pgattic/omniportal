#!/usr/bin/env bash
set -euo pipefail

PICO_RUSTUP_TOOLCHAIN="${PICO_RUSTUP_TOOLCHAIN:-stable}"
export RUSTUP_TOOLCHAIN="$PICO_RUSTUP_TOOLCHAIN"
unset CARGO_BUILD_TARGET
unset CARGO_UNSTABLE_BUILD_STD

rustup target add thumbv8m.main-none-eabihf >/dev/null

rustup run "$PICO_RUSTUP_TOOLCHAIN" cargo build \
  --release \
  --target thumbv8m.main-none-eabihf \
  --config 'target.thumbv8m.main-none-eabihf.rustflags=["-C","target-cpu=cortex-m33","-C","link-arg=-Tlink.x","-C","link-arg=-Llink/rp2350-pico2w","-C","link-arg=--nmagic"]'

if command -v picotool >/dev/null 2>&1; then
  picotool uf2 convert \
    target/thumbv8m.main-none-eabihf/release/omniportal \
    -t elf \
    target/thumbv8m.main-none-eabihf/release/omniportal-pico2w.uf2 \
    -t uf2 \
    --family 0xe48bff59 \
    --platform rp2350 \
    --abs-block
elif command -v elf2uf2-rs >/dev/null 2>&1; then
  elf2uf2-rs \
    target/thumbv8m.main-none-eabihf/release/omniportal \
    target/thumbv8m.main-none-eabihf/release/omniportal-pico2w.uf2
  python3 scripts/patch-rp2350-uf2-family.py \
    target/thumbv8m.main-none-eabihf/release/omniportal-pico2w.uf2
else
  echo "error: neither picotool nor elf2uf2-rs is available" >&2
  exit 1
fi
