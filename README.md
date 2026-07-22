
# OmniPortal

An ESP32-S3 toys-to-life base emulator for Skylanders and Disney Infinity.

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
scripts/build-firmware.sh
```

Run host-side unit tests:

```sh
omniportal-host-test
```

Flash and monitor:

```sh
scripts/flash-firmware.sh /dev/ttyACM0
```

## Code Structure

The project is still one package, but the shared library surface is separated
from ESP32-S3 firmware wiring:

* `lib.rs` - portable library entry point; host builds expose shared logic
* `main.rs` - thin ESP32-S3 firmware entry point
* `figures/` - figure identity and image helpers
* `storage/` - flash-backed catalog, records, and host-testable journal logic
* `usb/` - Skylanders and Disney Infinity USB descriptors and protocol helpers
* `web/` - HTTP parsing plus ESP32-S3 server wiring
* `dhcp.rs` - ESP32-S3 DHCPv4 server for AP clients
* `platform/esp32s3_n16r8/` - ESP32-S3 board entrypoint, WiFi, logging,
  flash adapter, heap setup, and board constants
* `config.rs` - temporary facade over platform board constants

Firmware-only modules are compiled for the Xtensa target. Host builds compile
the portable library logic for tests and future tooling.

USB mode is selected at boot from the persisted device config. Changing mode in
the web UI requires resetting or fully power-cycling the ESP32-S3 so the Wii
sees the new USB descriptor.

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

After connecting, the root page should show the OmniPortal control UI, and
`/status` should return current mode, active placement, and storage JSON.

## Storage Smoke Test

The firmware uses an explicit `omniportal` data partition for the append-only
journal:

* offset: `0x00e00000`
* size: `0x00200000`

The partition table lives at `partitions/esp32s3-n16r8.csv`. The factory app
partition remains at `0x10000` and is sized to end immediately before the
OmniPortal storage partition.

Console save writes are buffered in RAM while a toy is active. The firmware
flushes changed images on removal, mode switch/reset, and periodic checkpoints;
unchanged block writes are ignored. If the journal does not have enough space
for a save checkpoint, the firmware compacts storage automatically before
writing.

Useful read endpoints:

* `GET /api/library` - list identities and collection entities
* `GET /api/catalog` - list built-in catalog entries
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
curl -X POST -d 'mode=infinity' 'http://192.168.4.1/api/mode/set'
curl -X POST 'http://192.168.4.1/api/storage/compact'
```

Raw upload endpoints take query-string metadata and a binary request body:

```sh
curl -X POST --data-binary @toy.bin 'http://192.168.4.1/api/entity/upload?name=Imported+Toy'
```

Delete endpoints use POST as well:

```sh
curl -X POST -d 'id=1' 'http://192.168.4.1/api/entity/delete'
```

Collection entities are created from built-in catalogs or imported from raw
dumps. Skylanders entities use 1 KiB MIFARE images. Disney Infinity entities use
320-byte raw figure images. Some non-character toys are static collection items
when the game line does not need mutable save data on the toy.

The built-in Skylanders catalog is stored as typed Rust constants in
`src/figures/skylanders/catalog.rs`. The character IDs, variant IDs, names, and
categories were normalized from community reference material, primarily:

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

The firmware enumerates as either a Skylanders Portal of Power-compatible HID
device or a Disney Infinity Base-compatible HID device on the native USB
connector, depending on the saved USB mode at boot.

On Wii, the reliable flow is to select the desired mode, reset or power-cycle
the ESP32-S3, then launch the game. Disney Infinity has been observed to leave a
hot-plugged base suspended when the game is already waiting for a base, so
restart the game if it does not notice the device after a mode switch or replug.

Skylanders mode:

* VID/PID: `1430:0150`
* product string: `Portal of Power`
* interface: vendor-defined HID with 64-byte interrupt IN/OUT endpoints

Disney Infinity mode:

* VID/PID: `0e6f:0129`
* product string: `Disney Infinity Base`
* interface: vendor-defined HID with 32-byte interrupt IN/OUT endpoints

After flashing, plug the board's native USB connector into the host and run:

```sh
sudo python scripts/probe-skylanders-portal.py
```

The probe verifies that the device enumerates with the Skylanders VID/PID, that
the interrupt IN endpoint is present, that a HID `SET_REPORT` activate command
queues an `A 01 ff 77` response, and that `GET_REPORT` returns an `S` status
report.

For Disney Infinity mode, switch modes, fully reset or power-cycle the ESP32-S3,
then run:

```sh
sudo python scripts/probe-infinity-base.py
```
