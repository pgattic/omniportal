
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

* `dhcp.rs` - small DHCPv4 server for AP clients
* `web/` - HTTP routes and embedded UI
* `usb/` - future portal USB device modes
* `figures/` - future figure identity and image helpers
* `storage/` - flash-backed append-only catalog and blob records
* `platform/esp32s3_n16r8/` - ESP32-S3 board entrypoint, WiFi, logging,
  flash adapter, heap setup, and board constants
* `state.rs` - shared mode/selection state
* `config.rs` - temporary facade over platform board constants

The USB subsystem task is intentionally idle for now. WiFi/Web are active, and
storage scans a flash-backed append-only journal at boot. The GPIO blinky
remains the hardware runtime smoke test.

## WiFi AP Smoke Test

The firmware starts an open access point with DHCP:

* SSID: `Portal-Emulator`
* device IP: `192.168.4.1`
* DHCP pool: `192.168.4.100` through `192.168.4.199`
* HTTP: `http://192.168.4.1/`
* status JSON: `http://192.168.4.1/status`

Phones and laptops should be able to use automatic IP configuration. If a
client has cached an older failed connection attempt, forget the network and
join it again.

Manual fallback settings:

* IP address: `192.168.4.100`
* netmask: `255.255.255.0`
* gateway/router: `192.168.4.1`
* DNS: leave blank or use `192.168.4.1`

After connecting, the root page should show `Hello from ESP32-S3`, and
`/status` should return hardcoded JSON.

## Storage Smoke Test

The firmware reserves the final 256 KiB of the 16 MiB flash chip for the
OmniPortal append-only journal:

* offset: `0x00fc0000`
* size: `0x00040000`

This is a Phase B bootstrap layout based on the current boot log's factory app
partition ending at `0x00fb0000`. Before the firmware grows large, replace this
with an explicit partition-table entry.

Useful read endpoints:

* `GET /api/library` - list identities and instances
* `GET /api/identity/1.json` - download an identity sidecar
* `GET /api/instance/1.bin` - download an instance image
* `GET /api/backup/1.json` - download backup metadata
* `GET /api/backup/1.bin` - download a backup blob

Useful mutation endpoints:

```sh
curl -X POST 'http://192.168.4.1/api/storage/format'
curl -X POST -d 'name=Trigger+Happy&character_id=21' 'http://192.168.4.1/api/identity/create'
curl -X POST -d 'identity_id=1&name=Preston%27s+Trigger+Happy' 'http://192.168.4.1/api/instance/create'
curl -X POST -d 'source_id=1&name=Jacob%27s+Trigger+Happy' 'http://192.168.4.1/api/instance/clone'
curl -X POST -d 'id=1' 'http://192.168.4.1/api/instance/select'
curl -X POST -d 'id=1&name=Renamed+Trigger+Happy' 'http://192.168.4.1/api/identity/rename'
curl -X POST -d 'id=1&name=Renamed+Trigger+Happy' 'http://192.168.4.1/api/instance/rename'
curl -X POST -d 'id=1&name=Renamed+Backup' 'http://192.168.4.1/api/backup/rename'
curl -X POST 'http://192.168.4.1/api/instance/clear-active'
curl -X POST 'http://192.168.4.1/api/storage/compact'
```

Raw upload endpoints take query-string metadata and a binary request body:

```sh
curl -X POST --data-binary @figure.bin 'http://192.168.4.1/api/instance/upload?name=Imported+Trigger+Happy&identity_id=1'
curl -X POST --data-binary @backup.bin 'http://192.168.4.1/api/backup/upload?name=Raw+Trigger+Happy+Backup'
```

Delete endpoints use POST as well:

```sh
curl -X POST -d 'id=1' 'http://192.168.4.1/api/identity/delete'
curl -X POST -d 'id=1' 'http://192.168.4.1/api/instance/delete'
curl -X POST -d 'id=1' 'http://192.168.4.1/api/backup/delete'
```

Fresh Skylanders instances are currently placeholder 1 KiB images with an
OmniPortal marker and the character/variant IDs embedded. They prove durable
storage and exact download plumbing, but they are not yet valid game images.

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
