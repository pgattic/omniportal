# Wii Toys-to-Life Portal Emulator — Implementation Plan
### Platform: ESP32-S3-N16R8 · Rust · esp-hal / Embassy-style async

## 0. Project Summary

Build firmware for an ESP32-S3-N16R8 board that:
- Emulates a Skylanders "Portal of Power" OR a Disney Infinity Base over native USB to a Nintendo Wii (toggleable).
- Hosts its own open WiFi access point for the web UI; no existing router, SSID, password, or internet access is required.
- Stores figure identity records and collection entities in persistent flash; mutable entities keep save data, while static items/level pieces can be represented by catalog membership.
- Serves a phone-friendly web UI from the ESP32-S3 access point to import/export figure identities and save data, add catalog entities to the collection, select active figure(s), and toggle emulation mode.
- Persists console writes back into the selected mutable entity so character progress/save data survives power cycles.

No physical figurines or NFC readers are part of the finished device. Data acquisition can be as small as collecting the figure identity values needed to initialize a valid fresh image. Full raw dumps remain useful for import, but they should not be required for creating a new zero-progress entity.

Target module assumption: ESP32-S3-N16R8 means 16MB flash and 8MB PSRAM. The plan should still confirm the exact board pinout, USB connector wiring, boot mode behavior, and whether native USB D+/D- are exposed before buying or building hardware around it.

---

## 1. Feasibility Summary

This target is feasible, and in some ways cleaner than Pico 2 W:
- The ESP32-S3 has native full-speed USB device support and integrated WiFi on the same chip.
- 16MB flash is enough for firmware, embedded web assets, metadata, generated starter images, and a practical personal collection of small figure entities.
- Hosting an open AP is a natural fit for this product because the phone connects directly to the device near the Wii.

The main risk shifts from "can the board do this?" to software maturity and protocol details:
- Custom USB HID with console-sensitive timing is still the hardest part.
- Persistent flash storage needs careful wear management because games may write character data repeatedly.
- Skylanders is the better first protocol target because public information and emulator support are stronger.
- Disney Infinity remains higher risk because protocol references are thinner and may require traffic capture.
- Rust support on ESP32-S3 is workable, but expect more target-specific integration work than a pure desktop or Linux project.

Important data-model discovery: fresh figures do not need a known-good full save dump, but they also are not plain zero-filled files. The firmware needs per-game initializer logic that creates the required non-zero tag/card structure, identity fields, access/trailer blocks, keys, checksums, encrypted starter blocks where applicable, and then leaves character progress/save areas at their fresh defaults. Skylanders creation needs at least character ID + variant ID, plus an optional physical tag NUID if preserving the exact identity of a real figure matters. Disney Infinity creation needs the model/figure number and generated valid UID/encrypted starter data.

Definition of done for the target decision: basic firmware can simultaneously run WiFi AP + HTTP server, read/write persistent flash records, and enumerate as a USB HID device from the ESP32-S3 native USB port.

---

## 2. Prerequisites / Identity Acquisition

Goal: collect enough identity information for the firmware to generate valid fresh figure entities. Full dumps are optional import data, not the required path for new characters.

- [ ] Decide the supported identity input formats:
  - Skylanders: character ID, variant ID, display name, game line/type, and optional original tag NUID.
  - Disney Infinity: model/figure number, display name, type/slot category, and optional original UID if tooling exposes it.
- [ ] Provide a way to enter identities manually in the Web UI from known community lists.
- [ ] Provide binary/text upload for identity records so a PC-side tool can export a small identity file instead of a full save dump.
- [ ] Keep full raw dump upload support (`.bin`, `.sky`, or the Infinity community's common raw format) for users who want to preserve/import an existing figure's current progress.
- [ ] If using a PC NFC reader (separate from this project), use it to extract identity values and optional full raw dumps. ACR122U or PN532 USB dongles are reasonable choices.
- [ ] Validate generated fresh entities against Dolphin/PC tooling before treating the initializer as correct.

Definition of done: a set of identity records that can generate fresh zero-progress entities, plus optional raw dumps that can still be uploaded/imported.

---

## 3. Toolchain & Environment Setup

- [x] Install Rust via rustup.
- [x] Install the ESP Rust toolchain pieces required by the current `esp-hal`/`esp-wifi` stack for ESP32-S3.
- [x] Start from an ESP32-S3 Rust template that already boots on your board, initializes logging, and has the right linker/memory configuration.
- [x] Confirm the board can be flashed reliably over USB or UART.
- [x] Confirm native USB device mode is available on the board's exposed USB connector/pins. Avoid boards where the only USB connector is wired solely through a USB-UART bridge.
    - Plug into "COM" for flashing, "USB" for USB device mode
- [x] Blink an LED or print a serial log message as the first smoke test.
- [x] Add the minimal async/runtime setup needed by the chosen HAL stack.

Definition of done: trivial firmware builds, flashes, and runs on the exact ESP32-S3-N16R8 board.

---

## 4. Project Structure

Recommended crate layout (single binary crate to start; split into internal modules, not separate crates, to keep it simple):

    src/
      main.rs              - thin firmware entry point
      lib.rs               - shared library entry point for portable logic and host tests
      platform/
        mod.rs              - selected platform exports used by shared modules
        esp32s3_n16r8/
          mod.rs            - ESP32-S3 board entry point, task spawning, heap/timer/WiFi setup
          board.rs          - AP SSID/IP, flash region, and board constants
          wifi.rs           - ESP WiFi init, open AP setup, network stack bring-up
          storage_flash.rs  - ESP flash adapter used by storage
          log.rs            - ESP logging export
      web/
        mod.rs             - HTTP server setup
        routes.rs           - route handlers (upload/download identity, add entity, upload/download entity, clone entity, rename, select, toggle mode, status)
        ui_html.rs          - embedded HTML/CSS/JS for the control page
      usb/
        mod.rs              - shared USB device state machine, mode toggle logic
        skylanders.rs        - Skylanders portal descriptors + protocol handling
        infinity.rs          - Disney Infinity base descriptors + protocol handling
      figures/
        mod.rs               - figure library model, identity records, validation, lookup
        init.rs              - fresh figure image generation per game line
        formats.rs           - binary size/format helpers per supported game line
      storage/
        mod.rs               - persistent flash catalog + binary blob storage
        records.rs           - stored identity/entity metadata structures
        wear.rs              - deferred writes, journal/GC helpers
      state.rs               - shared app state (active figure, active mode)
      config.rs              - temporary facade over platform board constants

Definition of done: project compiles with empty/stub modules wired together; task skeleton runs.

---

## 5. Phase Breakdown

### Phase A — Open WiFi AP + Web Server Bring-Up (no USB yet)

- [x] Bring up ESP32-S3 WiFi in AP mode.
- [x] Use an open network for simplest phone access, no password.
- [x] Assign the device a predictable AP-side IP, for example `192.168.4.1`.
- [x] Enable DHCP for clients if the networking stack supports it directly. If DHCP is awkward in the first pass, document the phone's required manual IP settings and treat real DHCP as a follow-up task.
- [x] Stand up a minimal async HTTP server serving a single static "Hello" page.
- [x] Connect a phone directly to the ESP32-S3 AP and load the page.
- [x] Add a `/status` JSON endpoint returning a hardcoded placeholder (e.g. current mode, current figure).

Definition of done: phone connects to the ESP32-S3's own open WiFi network and loads a page served entirely by the device.

### Phase B — Persistent Storage Bring-Up (no USB protocol yet)

- [x] Choose the storage layout for flash: reserved partition/region, append-only journal or small embedded filesystem, metadata records, binary blob records, and garbage collection strategy.
- [x] Add an explicit ESP32-S3 partition-table entry for OmniPortal storage instead of relying on an undocumented free flash gap.
- [x] Define the data model:
  - Figure identity: game line, display name, character/model ID, variant ID if applicable, optional physical tag UID/NUID, type/slot category, source notes, checksum.
  - Entity: named collection entry generated from a catalog type or uploaded directly, data mode (`static-generated` or `mutable-image`), parent identity ID if any, creation/update timestamps or monotonic counters if available, checksum.
- [x] Add upload/download endpoints for identity records.
- [x] Add upload endpoints for direct entity binary files.
- [x] Add download endpoints for exact binary export of entities.
- [x] Add list/delete/rename endpoints for identities and entities.
- [ ] Add proper fresh entity initialization with zero-progress/default-progress state.
- [x] Add "clone entity" support that copies an existing image when the user wants another save slot from the same current state.
- [x] Add a small integrity check at boot: scan records, reject corrupt entries, expose storage status in `/status`.
- [ ] Confirm uploads survive reboot and power loss during a non-active upload.

Definition of done: phone can create/import an identity, generate a named fresh entity from it, download the entity, reboot, and see the same library again.

Host-side storage/protocol tests run with `omniportal-host-test` from the Nix
development shell.

### Phase C — Minimal USB HID Device (no protocol logic yet)

- [x] Bring up native USB device mode on ESP32-S3.
- [x] Implement a bare-bones custom HID device: arbitrary VID/PID, minimal report descriptor, no real command handling.
- [x] Plug into a PC first, not the Wii, and confirm it enumerates (`lsusb` / Device Manager should show it).
- [x] Confirm you can send/receive a raw HID report from a simple PC-side test script (Python + `hidapi` is the easiest path here).
- [x] Keep WiFi AP running while USB is active to prove the two subsystems can coexist.

Definition of done: PC recognizes the ESP32-S3 as a generic HID device while the web UI remains reachable over the ESP32-S3 AP.

### Phase D — Skylanders Protocol Implementation

- [x] Implement the Skylanders device descriptor from Dolphin: VID `0x1430`, PID `0x0150`, HID interface, interrupt endpoints, and control-transfer-based command channel (bmRequestType `0x21`, bRequest `0x09`) for commands.
- [x] Implement the status packet the portal continuously sends (`S` — figure-arrived events with slot IDs 10/11/... etc).
- [x] Implement command handling: `R` (activate/read), `A` (query), `Q` (read block), `W` (write block), `C` (LED color, can be ignored or just acked), `Z`.
  - [x] Initial command scaffold for `A`, `R`, `S`, `M`, `C`, `J`, `L`, `V`, `Z`, and no-figure `Q`/`W` responses.
  - [x] Back `Q`/`W` with the selected entity image and durable save writes.
- [x] Wire this up to the selected mutable entity: when the web UI marks an entity active, the USB task should emit the appropriate status packet as if that figure were just placed on the portal, answer `Q` block-reads from that entity's stored binary data, and apply `W` block-writes back to that entity.
- [x] Buffer writes in RAM during active gameplay and commit them to flash on a debounce/timer, on figure removal, and before mode changes to reduce flash wear.
- [x] Support multiple occupied portal slots in status reports, block reads, block writes, persisted active-slot config, and web placement controls.
- [ ] Implement Skylanders fresh-image generation from character ID + variant ID + optional NUID:
  - zero/default progress areas,
  - random or supplied NUID,
  - BCC, ATQA, SAK,
  - sector trailer permissions,
  - per-sector keys,
  - identity fields,
  - required checksum.
- [ ] Validate generated entity binary size and required fixed blocks before allowing selection.

Definition of done: a Skylanders game running on the Wii sees the selected named entity, can write progress to it, and that changed entity can be downloaded after reboot.

### Phase E — Web UI: Library Management + Figure Selection

- [ ] Build the actual control page:
  - Add/upload figure identity.
  - Add a catalog entity to the collection.
  - Upload existing playable entity.
  - Clone an existing entity.
  - Rename/delete identities and entities.
  - Select active entity.
  - Download identity record or entity image as a file.
  - Show current USB mode, selected entity, dirty/committed save status, and storage usage.
- [ ] Use binary upload/download formats that match existing archival tools wherever possible (`.bin`, `.sky`, or the Infinity community's common raw dump format) for full images. Store identity metadata separately so fresh generation does not require a full dump.
- [ ] Wire selection to update shared state (`state.rs`) that the USB task reads to decide what to report to the Wii.
- [ ] Confirm from your phone: add two mutable entities for the same figure, select one, change it in-game, download both, and verify only the active entity changed.

Definition of done: full loop works for Skylanders - connect to ESP32-S3 AP, add/import identity, add entities, name, select, persist console writes, and export files from the phone.

### Phase F — Disney Infinity Protocol Implementation

- [ ] Implement the Infinity base's device descriptor: VID `0x0e6f`, PID `0x0129`, HID device.
- [ ] Reverse-engineer/reference community docs for its command set — expect this to be less complete than Skylanders' docs, so budget time for USB traffic capture (Wireshark + USBPcap, or similar) against your own real base if you still have one, or against community writeups.
- [ ] Implement whatever subset of the protocol is needed for basic figure presence/read/write. Full base features like multi-figure positions and RGB lighting are stretch goals, not required for MVP.
- [ ] Reuse the same persistent identities/entities + shared-state plumbing from the Skylanders path, just behind the Infinity descriptor/command set instead.
- [ ] Implement Infinity fresh-image generation from model/figure number + optional UID:
  - standard NFC permissions,
  - random or supplied UID data,
  - generated AES key material,
  - encrypted character starter block,
  - encrypted blank/default blocks,
  - manufacture-date/default bytes,
  - CRC.
- [ ] Test on PC first if any Infinity-aware PC tooling exists; otherwise go straight to a real Infinity-compatible game on Wii, cautiously.

Definition of done: an Infinity game on the Wii sees the selected named entity and any supported console writes persist across reboot.

### Phase G — Mode Toggle (Skylanders ⇄ Infinity)

- [ ] Before toggling modes, flush any dirty active-entity data to flash.
- [ ] Implement a soft USB disconnect/reconnect: on mode toggle from the Web UI, disconnect the USB device, swap which descriptor set + command handler is active, then reconnect so the Wii re-enumerates the device type.
- [ ] Add a toggle control to the web UI, wired to this logic.
- [ ] Test toggling with the Wii already running the relevant game vs. toggling before launching the game — document which works reliably.
- [ ] Add a "please unplug and replug if the console doesn't notice" fallback note in the UI, since soft re-enumeration is not guaranteed on every host.

Definition of done: you can flip a switch in the web UI and have the Wii recognize the device as the other portal type without a firmware reflash, or you have documented the required unplug/replug fallback.

### Phase H — Import/Export Compatibility

- [ ] Test imports from the actual PC tools you plan to use for archival dumps.
- [ ] Test exports by loading downloaded files back into PC tools/emulators.
- [ ] Test generated fresh images by loading downloaded files back into PC tools/emulators.
- [ ] Decide whether the Web UI should accept metadata/identity sidecars later, or whether all metadata should remain user-entered on-device.
- [ ] Add duplicate detection using checksum and, where understood, character ID / model ID / UID / NUID / variant fields.
- [ ] Add storage capacity warnings before uploads or clone operations that would exceed the reserved flash region.

Definition of done: the device can generate fresh files from identities, round-trip real-world dump files through upload, mutate saves through gameplay, download, and verify externally.

### Phase I — Polish

- [ ] Improve web UI styling/usability for phone screens.
- [ ] Add basic error/status feedback in the UI (e.g. "USB not connected to console," "AP client connected," "mode change pending reconnect," "active entity has unsaved flash changes").
- [ ] Write a short personal README covering: flashing instructions, AP SSID/IP, identity entry/import, fresh entity generation, import/download workflow, how to create named entities, known quirks per game.
- [ ] Clean up logging so normal operation is quiet but debugging is still possible.
- [ ] Consider adding optional WPA2 later if open AP behavior is inconvenient in your environment.
- [ ] Add export-all support once single-file import/export is solid.

Definition of done: the device is comfortable to use day-to-day without needing to look at logs or re-derive how it works.

---

## 6. Key Dependencies (Cargo.toml, high level)

Exact crate names and features should be pinned when the ESP32-S3 template is chosen, but expect:

- `esp-hal` — ESP32-S3 HAL
- `esp-backtrace` / logging crate stack used by the chosen template
- `esp-wifi` — WiFi AP support
- `smoltcp` or the network stack used by `esp-wifi`
- `embassy-executor`, `embassy-time`, `embassy-sync` — async runtime + shared state primitives if using the Embassy path
- `embassy-usb` or a TinyUSB-backed binding — USB device stack capable of custom HID and control transfer handling
- `picoserve` or another no-std/embedded-friendly HTTP server
- Flash storage support from the chosen ESP stack, plus a small embedded filesystem or purpose-built append-only record store
- `static_cell`, `heapless`, `portable-atomic` or similar embedded support crates as needed

Before committing to the USB crate, prove it can handle the Skylanders control-transfer command path. Generic HID report support alone is not enough.

---

## 7. Cross-Cutting Concerns / Risks to Track

- USB timing sensitivity: real consoles can be less forgiving than PC hosts of slow or malformed responses — validate protocol changes against a PC tool before testing on the Wii.
- USB stack fit: make sure the chosen ESP32-S3 Rust USB stack supports custom descriptors, interrupt endpoints, and HID class/control request handling at the level this project needs.
- WiFi AP behavior: open AP mode is simpler for users, but phones may warn that the network has no internet. The UI should still work after joining the AP.
- DHCP/captive portal polish: DHCP is important for easy use. Captive-portal-style redirect is nice-to-have, not MVP.
- Persistent save correctness: mutable entities should be treated as binary images. Console writes need bounds checks, checksums/validation where applicable, and durable commits.
- Flash wear: avoid writing flash for every USB write command. Coalesce writes and use a journal or wear-leveled storage strategy.
- Starting data: full known-good dumps are not required for fresh characters, but valid initializer logic is required. All-zero images are not accepted; generated images must include the correct identity/config/key/checksum/encryption blocks for the game line.
- Soft re-enumeration reliability (Phase G) is not guaranteed — treat it as a spike/experiment early.
- Infinity protocol documentation is thinner than Skylanders' — budget extra time/uncertainty for Phase F.
- Flash budget: generated figure images and imported dumps are small, and 16MB flash is comfortable for a practical collection, but every named mutable entity consumes its own copy unless deduplication or copy-on-write is added later.
- PSRAM is useful headroom for networking buffers and web responses, but the core design should not require large dynamic allocations.

---

## 8. Later Pico 2 W Support

Supporting Raspberry Pi Pico 2 W later is feasible, but it should be treated as a second hardware backend, not a trivial recompile.

Likely reusable with modest changes:
- Figure library data model, fresh-image generation, and import/export validation.
- Web routes and HTML/CSS/JS, if kept mostly platform-neutral.
- High-level app state model.
- Skylanders and Infinity protocol state machines, if the USB transport is abstracted cleanly.

Likely platform-specific:
- WiFi bring-up: ESP32-S3 uses integrated WiFi; Pico 2 W uses CYW43.
- AP + DHCP support details.
- USB peripheral setup and descriptor plumbing.
- Flash layout, persistence, boot/flashing flow, logging, and linker configuration.
- Timing and task scheduling details around USB polling/interrupts.

Expected work estimate after the ESP32-S3 version works:
- Minimal compile-and-boot Pico 2 W port: several focused sessions.
- AP + web UI parity: moderate work, mainly CYW43 AP mode and networking integration.
- USB parity: moderate to high work, depending on how cleanly the USB/device protocol code was separated.
- End-to-end Wii validation: still required; protocol logic reuse does not eliminate host-specific USB quirks.

Best way to keep this affordable: from the beginning, keep `wifi`, `usb`, flash storage, and board configuration behind narrow module boundaries. Do not let ESP-specific types leak into the figure database, web route logic, or portal protocol state machines.

---

## 9. Suggested Order of Work for an LLM Pairing Session

1. Phase A (ESP32-S3 open AP + web server) — proves the phone control path without console risk.
2. Phase B (persistent storage) — proves upload/download and reboot survival before USB writes depend on it.
3. Phase C (minimal USB HID while AP stays running) — proves the major hardware concurrency question.
4. Phase D (Skylanders protocol) — builds on USB + storage, test against PC tooling before Wii.
5. Phase E (Web UI library management) — builds on AP + storage + Skylanders.
6. Phase H (import/export compatibility) can happen in parallel once one real figure round-trips.
7. Phase F (Infinity protocol) — same pattern as Skylanders, once the storage model is proven.
8. Phase G (mode toggle) — only after both protocol implementations independently work.
9. Phase I (polish) — last.

Each phase above is scoped to be a self-contained unit of work with a clear definition of "done", suitable for handing to an LLM one phase at a time with the relevant prior code as context.
