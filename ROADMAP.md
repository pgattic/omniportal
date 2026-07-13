# Wii Toys-to-Life Portal Emulator — Implementation Plan
### Platform: ESP32-S3-N16R8 · Rust · esp-hal / Embassy-style async

## 0. Project Summary

Build firmware for an ESP32-S3-N16R8 board that:
- Emulates a Skylanders "Portal of Power" OR a Disney Infinity Base over native USB to a Nintendo Wii (toggleable).
- Hosts its own open WiFi access point for the web UI; no existing router, SSID, password, or internet access is required.
- Has all figure data pre-baked into firmware at compile time (no runtime NFC hardware).
- Serves a phone-friendly web UI from the ESP32-S3 access point to select the active figure(s) and toggle emulation mode.
- Lets you download raw figure data backups through that same web UI.

No physical figurines or NFC readers are part of the finished device. Data acquisition (dumping real figures) is a one-time, separate, PC-side task done before firmware is built.

Target module assumption: ESP32-S3-N16R8 means 16MB flash and 8MB PSRAM. The plan should still confirm the exact board pinout, USB connector wiring, boot mode behavior, and whether native USB D+/D- are exposed before buying or building hardware around it.

---

## 1. Feasibility Summary

This target is feasible, and in some ways cleaner than Pico 2 W:
- The ESP32-S3 has native full-speed USB device support and integrated WiFi on the same chip.
- 16MB flash is generous for firmware, embedded web assets, and a personal collection of small figure dumps.
- Hosting an open AP is a natural fit for this product because the phone connects directly to the device near the Wii.

The main risk shifts from "can the board do this?" to software maturity and protocol details:
- Custom USB HID with console-sensitive timing is still the hardest part.
- Skylanders is the better first protocol target because public information and emulator support are stronger.
- Disney Infinity remains higher risk because protocol references are thinner and may require traffic capture.
- Rust support on ESP32-S3 is workable, but expect more target-specific integration work than a pure desktop or Linux project.

Definition of done for the target decision: basic firmware can simultaneously run WiFi AP + HTTP server and enumerate as a USB HID device from the ESP32-S3 native USB port.

---

## 2. Prerequisites / Data Acquisition (do this first, outside the firmware project)

Goal: get raw dumps of every figure you own into flat binary files before writing any firmware logic.

- [ ] Acquire a cheap USB NFC reader for your PC (e.g. ACR122U or a PN532 USB dongle).
- [ ] Use existing community tooling (SkyReader-derived tools, or Dolphin emulator's built-in Skylanders portal/figure tooling) to dump each figure to a `.bin`/`.sky` file.
- [ ] Do the same for any Disney Infinity figures using available Infinity-focused community tools.
- [ ] Organize dumps into a folder structure, one file per figure, clearly named.
- [ ] Write down for each figure: display name, character ID / variant ID (if the tool reports it), and which game line it belongs to (Skylanders vs Infinity).
- [ ] Verify each dump by reading it back with the same PC tool before trusting it — a bad one-time dump means a bad figure forever once baked into firmware.

Definition of done: a folder of verified raw figure dumps + a small manifest mapping filename → figure name → game line.

---

## 3. Toolchain & Environment Setup

- [ ] Install Rust via rustup.
- [ ] Install the ESP Rust toolchain pieces required by the current `esp-hal`/`esp-wifi` stack for ESP32-S3.
- [ ] Start from an ESP32-S3 Rust template that already boots on your board, initializes logging, and has the right linker/memory configuration.
- [ ] Confirm the board can be flashed reliably over USB or UART.
- [ ] Confirm native USB device mode is available on the board's exposed USB connector/pins. Avoid boards where the only USB connector is wired solely through a USB-UART bridge.
- [ ] Blink an LED or print a serial log message as the first smoke test.
- [ ] Add the minimal async/runtime setup needed by the chosen HAL stack.

Definition of done: trivial firmware builds, flashes, and runs on the exact ESP32-S3-N16R8 board.

---

## 4. Project Structure

Recommended crate layout (single binary crate to start; split into internal modules, not separate crates, to keep it simple):

    src/
      main.rs              - entry point, task spawning
      wifi.rs              - ESP WiFi init, open AP setup, network stack bring-up
      web/
        mod.rs             - HTTP server setup
        routes.rs           - route handlers (select figure, toggle mode, download backup, status)
        ui_html.rs          - embedded HTML/CSS/JS for the control page
      usb/
        mod.rs              - shared USB device state machine, mode toggle logic
        skylanders.rs        - Skylanders portal descriptors + protocol handling
        infinity.rs          - Disney Infinity base descriptors + protocol handling
      figures/
        mod.rs               - figure data table/index, lookup by ID
        data.rs              - generated/const byte arrays for each figure dump
      state.rs               - shared app state (active figure, active mode)
      config.rs              - AP SSID, static IP, build-time config values

Definition of done: project compiles with empty/stub modules wired together; task skeleton runs.

---

## 5. Phase Breakdown

### Phase A — Open WiFi AP + Web Server Bring-Up (no USB yet)

- [ ] Bring up ESP32-S3 WiFi in AP mode.
- [ ] Use an open network for simplest phone access, for example SSID `Portal-Emulator`, no password.
- [ ] Assign the device a predictable AP-side IP, for example `192.168.4.1`.
- [ ] Enable DHCP for clients if the networking stack supports it directly. If DHCP is awkward in the first pass, document the phone's required manual IP settings and treat real DHCP as a follow-up task.
- [ ] Stand up a minimal async HTTP server serving a single static "Hello" page.
- [ ] Connect a phone directly to the ESP32-S3 AP and load the page.
- [ ] Add a `/status` JSON endpoint returning a hardcoded placeholder (e.g. current mode, current figure).

Definition of done: phone connects to the ESP32-S3's own open WiFi network and loads a page served entirely by the device.

### Phase B — Minimal USB HID Device (no protocol logic yet)

- [ ] Bring up native USB device mode on ESP32-S3.
- [ ] Implement a bare-bones custom HID device: arbitrary VID/PID, minimal report descriptor, no real command handling.
- [ ] Plug into a PC first, not the Wii, and confirm it enumerates (`lsusb` / Device Manager should show it).
- [ ] Confirm you can send/receive a raw HID report from a simple PC-side test script (Python + `hidapi` is the easiest path here).
- [ ] Keep WiFi AP running while USB is active to prove the two subsystems can coexist.

Definition of done: PC recognizes the ESP32-S3 as a generic HID device while the web UI remains reachable over the ESP32-S3 AP.

### Phase C — Skylanders Protocol Implementation

- [ ] Implement the exact device descriptor: VID `0x1430`, PID `0x0150`, HID device, single interrupt-IN endpoint for status, control-transfer-based command channel (bmRequestType `0x21`, bRequest `0x09`) for commands, both 32 bytes, zero-padded.
- [ ] Implement the status packet the portal continuously sends (`S` — figure-arrived events with slot IDs 10/11/... etc).
- [ ] Implement command handling: `R` (activate/read), `A` (query), `Q` (read block), `W` (write block — can be a no-op/ack-only stub since you don't need writeback), `C` (LED color, can be ignored or just acked), `Z`.
- [ ] Wire this up to the `figures` module: when the web UI marks a figure active, the USB task should emit the appropriate status packet as if that figure were just placed on the portal, and answer subsequent `Q` block-reads with that figure's stored data.
- [ ] Test against a PC-side Skylanders-aware tool first (e.g. Dolphin's Skylanders portal support, or SkyReader-based tooling) before using the real Wii.
- [ ] Once PC-side testing passes, test on the actual Wii with a real Skylanders game.

Definition of done: a Skylanders game running on the Wii sees your baked-in figure appear on the portal when selected from the web UI.

### Phase D — Web UI: Figure Selection + Backups

- [ ] Build the actual control page: list of baked-in figures (name, game line), a "make active" action per figure, and current status display.
- [ ] Wire the "make active" action to update shared state (`state.rs`) that the USB task reads to decide what to report to the Wii.
- [ ] Add a per-figure "download backup" link/button that streams the figure's raw stored bytes back as a file download (e.g. `Content-Disposition: attachment`).
- [ ] Confirm from your phone: select a figure, verify it shows up in-game; download a backup, verify the file matches the original dump byte-for-byte.

Definition of done: full loop works for Skylanders — connect to ESP32-S3 AP, select in web UI, figure appears on Wii, backup downloads from phone.

### Phase E — Disney Infinity Protocol Implementation

- [ ] Implement the Infinity base's device descriptor: VID `0x0e6f`, PID `0x0129`, HID device.
- [ ] Reverse-engineer/reference community docs for its command set — expect this to be less complete than Skylanders' docs, so budget time for USB traffic capture (Wireshark + USBPcap, or similar) against your own real base if you still have one, or against community writeups.
- [ ] Implement whatever subset of the protocol is needed for basic figure presence/read. Full base features like multi-figure positions and RGB lighting are stretch goals, not required for MVP.
- [ ] Reuse the same `figures` data + shared-state plumbing from Phase C/D, just behind the Infinity descriptor/command set instead.
- [ ] Test on PC first if any Infinity-aware PC tooling exists; otherwise go straight to a real Infinity-compatible game on Wii, cautiously.

Definition of done: an Infinity game on the Wii sees a baked-in figure appear when selected from the web UI.

### Phase F — Mode Toggle (Skylanders ⇄ Infinity)

- [ ] Implement a soft USB disconnect/reconnect: on mode toggle from the web UI, disconnect the USB device, swap which descriptor set + command handler is active, then reconnect so the Wii re-enumerates the device type.
- [ ] Add a toggle control to the web UI, wired to this logic.
- [ ] Test toggling with the Wii already running the relevant game vs. toggling before launching the game — document which works reliably.
- [ ] Add a "please unplug and replug if the console doesn't notice" fallback note in the UI, since soft re-enumeration is not guaranteed on every host.

Definition of done: you can flip a switch in the web UI and have the Wii recognize the device as the other portal type without a firmware reflash, or you have documented the required unplug/replug fallback.

### Phase G — Figure Data Integration (bulk)

- [ ] Convert your full folder of verified dumps into Rust byte arrays — either via a small build script that generates `data.rs` from the binary files, or with `include_bytes!` per file.
- [ ] Build the figure index/table (`figures/mod.rs`) mapping an internal ID → name → game line → byte slice.
- [ ] Regenerate and reflash firmware with the full set; re-test a handful of figures end-to-end.
- [ ] Check final firmware size against the 16MB flash budget.

Definition of done: your entire real-world figure collection is represented and selectable in the web UI.

### Phase H — Polish

- [ ] Persist last-selected figure/mode across power cycles if desired using a small flash-backed key-value record or reserved flash page.
- [ ] Improve web UI styling/usability for phone screens.
- [ ] Add basic error/status feedback in the UI (e.g. "USB not connected to console," "AP client connected," "mode change pending reconnect").
- [ ] Write a short personal README covering: flashing instructions, AP SSID/IP, how to add a new figure (requires reflash), known quirks per game.
- [ ] Clean up logging so normal operation is quiet but debugging is still possible.
- [ ] Consider adding optional WPA2 later if open AP behavior is inconvenient in your environment.

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
- `static_cell`, `heapless`, `portable-atomic` or similar embedded support crates as needed

Before committing to the USB crate, prove it can handle the Skylanders control-transfer command path. Generic HID report support alone is not enough.

---

## 7. Cross-Cutting Concerns / Risks to Track

- USB timing sensitivity: real consoles can be less forgiving than PC hosts of slow or malformed responses — validate protocol changes against a PC tool before testing on the Wii.
- USB stack fit: make sure the chosen ESP32-S3 Rust USB stack supports custom descriptors, interrupt endpoints, and HID class/control request handling at the level this project needs.
- WiFi AP behavior: open AP mode is simpler for users, but phones may warn that the network has no internet. The UI should still work after joining the AP.
- DHCP/captive portal polish: DHCP is important for easy use. Captive-portal-style redirect is nice-to-have, not MVP.
- Soft re-enumeration reliability (Phase F) is not guaranteed — treat it as a spike/experiment early.
- Infinity protocol documentation is thinner than Skylanders' — budget extra time/uncertainty for Phase E.
- Flash budget: figure dumps are small, and 16MB flash is comfortable, but check actual total size once all dumps are collected.
- PSRAM is useful headroom for networking buffers and web responses, but the core design should not require large dynamic allocations.

---

## 8. Later Pico 2 W Support

Supporting Raspberry Pi Pico 2 W later is feasible, but it should be treated as a second hardware backend, not a trivial recompile.

Likely reusable with modest changes:
- Figure manifest/data generation.
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
2. Phase B (minimal USB HID while AP stays running) — proves the major hardware concurrency question.
3. Phase C (Skylanders protocol) — builds on B, test against PC tooling before Wii.
4. Phase D (web UI wiring) — builds on A + C.
5. Phase G (bulk figure data) can happen in parallel with C/D once the single-figure path works.
6. Phase E (Infinity protocol) — same pattern as C, once C/D are proven.
7. Phase F (mode toggle) — only after both protocol implementations independently work.
8. Phase H (polish) — last.

Each phase above is scoped to be a self-contained unit of work with a clear definition of "done", suitable for handing to an LLM one phase at a time with the relevant prior code as context.
