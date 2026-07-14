# USB Target Behavior

This captures the first USB target contract for OmniPortal. The initial
implementation target is Skylanders Portal of Power behavior. Disney Infinity
stays a later mode behind the same higher-level storage and selection model.

Reference: the local Dolphin checkout under
`dolphin/Source/Core/Core/IOS/USB/Emulated/Skylanders/`, especially
`Skylander.cpp` and `Skylander.h`.

## Initial Target

Start with Skylanders because the protocol is better represented in Dolphin and
the project storage model already stores Skylanders-style 1 KiB figure images.

The minimal USB bring-up in Phase C may enumerate as a generic HID while proving
ESP32-S3 native USB and WiFi coexist. The protocol target for Phase D is the
Skylanders descriptor and packet behavior below.

## Skylanders Descriptors

Device descriptor:

* USB version: `0x0200`
* class/subclass/protocol: `0x00` / `0x00` / `0x00`
* EP0 max packet size: 64
* VID/PID: `0x1430` / `0x0150`
* device version: `0x0100`
* manufacturer string index: 1
* product string index: 2
* serial string index: 0
* configurations: 1

Configuration:

* total length: `0x0029`
* interfaces: 1
* configuration value: 1
* attributes: `0x80`
* max power: 500 mA

Interface 0:

* alternate setting: 0
* endpoints: 2
* class: HID (`0x03`)
* subclass/protocol: `0x00` / `0x00`

Endpoints:

* `0x81`: interrupt IN, max packet 64, interval 1 ms
* `0x02`: interrupt OUT, max packet 64, interval 1 ms

## Transport

Commands are sent over HID class control requests:

* `bmRequestType = 0x21`
* `bRequest = 0x09`
* command payloads are byte commands, generally 32 bytes zero-padded

Most commands get a small immediate control-transfer acknowledgement and, when
they return portal data, queue a 32-byte interrupt-IN report. If no queued
command response is waiting, interrupt IN returns the current portal status.

For firmware architecture, model this as:

1. Control SET_REPORT parses a command and pushes zero or one response report
   into a USB response queue.
2. Interrupt IN first drains that queue.
3. Interrupt IN otherwise emits a status report.

## Reports And Commands

All normal portal reports are 32 bytes. Bytes not listed are zero.

`A` activate/deactivate:

* command: `A, active`
* queued report: `A, active, 0xff, 0x77`
* active is `0x01` for active and `0x00` for inactive

`R` ready:

* command: `R, 0x00`
* queued report: `R, 0x02, 0x1b`

`S` status:

* command acknowledgement is immediate
* status reports are produced by interrupt IN
* report: `S, status[0..4 little-endian], counter, active`
* each figure slot consumes two bits in the 32-bit status field
* slot states: removed `0`, ready `1`, removing `2`, added `3`
* active is `0x01` when the portal is activated

`Q` read block:

* command: `Q, slot_id, block`
* slot IDs start at `0x10`; slot number is `slot_id & 0x0f`
* valid blocks are `0..63`
* success report: `Q, slot_id, block, 16 bytes of block data`
* error report: `Q, 0x01, block`

`W` write block:

* command: `W, slot_id, block, 16 bytes of block data`
* success report: `W, slot_id, block`
* error report: `W, 0x01, block`
* firmware should update RAM first and persist through the storage debounce path

LED/audio-related commands can be accepted without affecting MVP behavior:

* `C`: global RGB color, may be acknowledged and ignored
* `J`: sided RGB color, may be acknowledged and ignored
* `L`: light command used around portal audio, may be acknowledged and ignored
* `M`: audio firmware version, Dolphin uses `M, requested, 0x00, 0x19`
* `V`: echo-style acknowledgement

## Active Instance Mapping

For the first real Skylanders implementation:

* The selected storage instance maps to portal slot 0, exposed as slot ID
  `0x10`.
* Selecting an instance should queue slot status transitions `ADDED`, then
  `READY`.
* Clearing or changing the active instance should queue `REMOVING`, then
  `REMOVED` for the old slot.
* Reads use the selected instance's 1024-byte image as 64 blocks of 16 bytes.
* Writes mutate an in-RAM copy, mark the instance dirty, and are committed to
  flash on debounce, removal, mode change, and orderly shutdown paths.

## Open Questions

* Which Rust USB stack gives enough control over HID class SET_REPORT handling
  on ESP32-S3: `embassy-usb`, TinyUSB bindings, or a lower-level ESP HAL USB
  device path?
* Whether Wii titles require exact string descriptors or HID report descriptor
  bytes beyond the descriptor fields above.
* Whether the interrupt OUT endpoint is required for all target games, or only
  for portal audio/newer titles.
