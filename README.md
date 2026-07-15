
# OmniPortal

A modern Skylanders portal simulator for PS3, PS4, Wii and Wii U

## ESP32-S3 Firmware

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

Run host-side unit tests:

```sh
cargo test -Z build-std=std --target "$(rustc -vV | sed -n 's/^host: //p')" --lib
```

Flash and monitor:

```sh
espflash flash --monitor target/xtensa-esp32s3-none-elf/release/omniportal
```

## Code Structure

The project is still one package, but the shared library surface is separated
from ESP32-S3 firmware wiring:

* `lib.rs` - portable library entry point; host builds expose shared logic
* `main.rs` - thin ESP32-S3 firmware entry point
* `figures/` - figure identity and image helpers
* `storage/` - flash-backed catalog, records, and host-testable journal logic
* `usb/` - portal protocol constants and packet helpers
* `web/` - HTTP parsing plus ESP32-S3 server wiring
* `state.rs` - shared mode/selection state
* `dhcp.rs` - ESP32-S3 DHCPv4 server for AP clients
* `platform/esp32s3_n16r8/` - ESP32-S3 board entrypoint, WiFi, logging,
  flash adapter, heap setup, and board constants
* `config.rs` - temporary facade over platform board constants

Firmware-only modules are compiled for the Xtensa target. Host builds compile
the portable library logic for tests and future tooling.

The USB subsystem task is intentionally idle for now. WiFi/Web are active, and
storage scans a flash-backed append-only journal at boot.

The planned USB target behavior is documented in
[`docs/usb-target.md`](docs/usb-target.md).

## WiFi AP Smoke Test

The firmware starts an open access point with DHCP:

* SSID: `OmniPortal`
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

* `GET /api/library` - list identities and collection entities
* `GET /api/catalog` - list built-in Skylanders catalog entries
* `GET /api/identity/1.json` - download an identity sidecar
* `GET /api/entity/1.bin` - download an entity image

Useful mutation endpoints:

```sh
curl -X POST 'http://192.168.4.1/api/storage/format'
curl -X POST -d 'catalog_index=0&name=Trigger+Happy' 'http://192.168.4.1/api/entity/create-from-catalog'
curl -X POST -d 'source_id=1&name=Second+Save+Slot' 'http://192.168.4.1/api/entity/clone'
curl -X POST -d 'id=1' 'http://192.168.4.1/api/entity/select'
curl -X POST -d 'id=1&name=Renamed+Trigger+Happy' 'http://192.168.4.1/api/entity/rename'
curl -X POST 'http://192.168.4.1/api/entity/clear-active'
curl -X POST 'http://192.168.4.1/api/storage/compact'
```

Raw upload endpoints take query-string metadata and a binary request body:

```sh
curl -X POST --data-binary @figure.bin 'http://192.168.4.1/api/entity/upload?name=Imported+Figure'
```

Delete endpoints use POST as well:

```sh
curl -X POST -d 'id=1' 'http://192.168.4.1/api/entity/delete'
```

Collection entities are created from the built-in Skylanders catalog. Characters,
traps, creation crystals, vehicles, and trophies get mutable 1 KiB placeholder
images for now. Items and level pieces are stored as static-generated collection
entries and synthesize their placeholder image only when downloaded or cloned.
The placeholder images prove durable storage and exact download plumbing, but
they are not yet valid game images.

The built-in Skylanders catalog is stored as typed Rust constants in
`src/figures/catalog.rs`. The character IDs, variant IDs, names, and categories
were normalized from community reference material, primarily:

* <https://github.com/Texthead1/Skylander-IDs>
* <https://github.com/NefariousTechSupport/Runes/blob/master/Docs/SkylanderFormat.md>

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
