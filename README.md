
# OmniPortal

A modern Skylanders portal simulator for PS3, PS4, Wii and Wii U

## ESP32-S3 Blinky

Enter the Nix development shell:

```sh
nix develop --builders ''
```

Bootstrap the ESP Rust toolchain locally:

```sh
espup install --targets esp32s3 --export-file "$PWD/export-esp.sh"
scripts/patch-esp-toolchain-nixos.sh
```

Build the firmware:

```sh
cargo build --release
```

Flash and monitor:

```sh
espflash flash --monitor target/xtensa-esp32s3-none-elf/release/omniportal
```

The default blink pin is GPIO2. The monitor prints the selected GPIO at startup:

```text
Blinking GPIO2
```

The common dual-USB ESP32-S3-N16R8 board shown in `photo.jpg` does not have a
firmware-controllable RGB LED despite the nearby `RGB` label. Those three LEDs
are board status indicators:

* red: power
* green: USB-serial TX activity
* blue: USB-serial RX activity

The green LED will flash when firmware writes to the serial monitor, and the
blue LED can flash while firmware is being uploaded. For a visible firmware
blinky on this board, connect an external LED and resistor from GPIO2 to GND,
or probe GPIO2 with a meter/logic analyzer.

To test GPIO48 instead:

```sh
cargo build --release --no-default-features --features led-gpio-48
espflash flash --monitor target/xtensa-esp32s3-none-elf/release/omniportal
```

## Firmware Structure

The firmware is currently a single binary crate with subsystem stubs wired into
the entry point:

* `wifi.rs` - future ESP32-S3 AP/network bring-up
* `web/` - future HTTP routes and embedded UI
* `usb/` - future portal USB device modes
* `figures/` - future figure identity and image helpers
* `storage/` - future flash records and wear-management helpers
* `state.rs` - shared mode/selection state
* `config.rs` - build-time constants

The stub subsystem tasks are intentionally idle for now; the GPIO blinky remains
the runtime smoke test.

## Native USB Check

This board has two USB-C connectors. One may be wired through a USB-UART bridge,
while the other may expose the ESP32-S3 native USB peripheral. For the portal
emulator, the native USB connector is the important one.

On the host, watch USB events:

```sh
sudo dmesg -w
```

In another terminal, compare devices before and after plugging into each USB-C
connector:

```sh
lsusb
ls -l /dev/serial/by-id/
```

Useful signs:

* `303a:1001` / `Espressif USB JTAG/serial debug unit` means the ESP32-S3
  native USB peripheral is exposed.
* `10c4:ea60` / `CP210x`, `1a86:55d4` / `CH343`, or similar means that connector
  is a USB-UART bridge, not native USB device mode.

If the native connector appears as an Espressif USB device, then the board has
the wiring needed for later HID-device firmware.
