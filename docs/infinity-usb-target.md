# Disney Infinity USB Target Behavior

This captures the first implementation contract for OmniPortal's Disney
Infinity mode. The reference is the local Dolphin checkout:

`dolphin/Source/Core/Core/IOS/USB/Emulated/Infinity.cpp` and
`dolphin/Source/Core/Core/IOS/USB/Emulated/Infinity.h`.

## Descriptors

Device descriptor:

* USB version: `0x0200`
* class/subclass/protocol: `0x00` / `0x00` / `0x00`
* EP0 max packet size: 32
* VID/PID: `0x0e6f` / `0x0129`
* device version: `0x0200`
* manufacturer string index: 1
* product string index: 2
* serial string index: 3
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

* `0x81`: interrupt IN, max packet 32, interval 1 ms
* `0x01`: interrupt OUT, max packet 32, interval 1 ms

## Transport

Dolphin models Infinity commands as 32-byte interrupt transfers rather than the
Skylanders HID control-transfer command path.

Observed host-to-device packet types:

* `00 ...`: first call / constant device response path.
* `aa ...` or `ab ...`: host polls for queued response data. If a figure
  add/remove response is queued, it is returned first. Otherwise a queued command
  response is returned. If nothing is queued, Dolphin parks the interrupt
  transfer until data is available.
* `ff len command sequence [payload...] checksum`: command packet. `len` counts
  `command`, `sequence`, and payload bytes, but not `ff` or checksum. The
  checksum is the low byte of the sum of all bytes before it, including `ff` and
  `len`.

Dolphin schedules two responses for `ff` commands:

1. An echo of the 32-byte command packet.
2. The command-specific response, returned immediately if an interrupt-IN poll is
   waiting or queued for the next `aa` / `ab` poll.

## Commands

All normal response frames are 32 bytes. Bytes not listed are zero.

`0x80` activate base:

* response starts with:
  `aa 15 00 00 0f 01 00 03 02 09 09 43 20 32 62 36 36 4b 34 99 67 31 93 8c`
* Dolphin's constant response does not include the request sequence.

`0x81` seed RNG:

* payload: 8-byte scrambled seed
* response: blank response with the request sequence
* also seeds the pseudo-random stream used by command `0x83`

`0x83` get random:

* response: `aa 09 sequence <8 scrambled bytes> checksum`

`0x90`, `0x92`, `0x93`, `0x95`, `0x96` LED/color commands:

* response: blank response with the request sequence
* MVP can ignore actual LED behavior.

`0xa1` get present figures:

* response: `aa len sequence [position_order 09]... checksum`
* For each present figure, two bytes are emitted.
* Base position byte is `0x10` for the hexagonal slot, `0x20` for player 1, and
  `0x30` for player 2, plus the figure's order-added value.
* Empty base response has no figure payload.

`0xa2` read figure data:

* payload: `order block unknown`
* `order` is the figure order-added value.
* `block` maps to file block 1 when `0`; otherwise file block `block * 4`.
* response: `aa 12 sequence 00 <16 bytes> checksum`
* Infinity images are 20 blocks of 16 bytes, 320 bytes total.

`0xa3` write figure data:

* payload: `order block unknown <16 bytes>`
* block mapping matches `0xa2`
* response: `aa 02 sequence 00 checksum`

`0xb4` get tag ID:

* payload: `order`
* response: `aa 09 sequence 00 <7-byte UID> checksum`

`0xb5` status-like command:

* response: blank response with the request sequence

## Base Positions

Dolphin has nine UI positions:

* Hexagon disc positions: 0, 1, 2
* Player 1 figure and ability discs: 3, 4, 5
* Player 2 figure and ability discs: 6, 7, 8

Those map to three physical base positions in add/remove and presence responses:

* `0x01`: hexagon slot
* `0x02`: player 1 slot
* `0x03`: player 2 slot

## Figure Image

Dolphin stores Disney Infinity figures as 320-byte images:

* 20 blocks
* 16 bytes per block
* block 0 starts with UID/tag data
* block 1 stores encrypted figure metadata
* blocks 4, 8, 12, and 13 are encrypted blank blocks in generated figures
* access/permission bytes are written at sector-trailer-style offsets

Fresh image creation needs AES/SHA1 and Infinity-specific CRC32. For OmniPortal,
raw import/export can be supported before generated fresh Infinity images are
implemented.

## Initial OmniPortal Target

The first firmware implementation should:

* enumerate with the descriptor constants above when Infinity mode is selected
* parse 32-byte interrupt OUT packets
* queue 32-byte interrupt IN responses
* answer activate, blank/status-like commands, presence, tag ID, read, and write
* keep RNG/auth commands explicit but allowed to return deterministic placeholder
  responses until console behavior demands exact implementation
* map shared collection placement to Infinity base positions instead of
  Skylanders protocol slots

