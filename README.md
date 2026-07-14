
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
