# Wii Toys-to-Life Portal Emulator — Implementation Plan
### Platform: Raspberry Pi Pico 2 W (RP2350) · Rust · embassy-rs

## 0. Project Summary

Build firmware for a Pico 2 W that:
- Emulates a Skylanders "Portal of Power" OR a Disney Infinity Base over USB to a Nintendo Wii (toggleable).
- Has all figure data pre-baked into firmware at compile time (no runtime NFC hardware).
- Serves a web UI over WiFi (from your phone) to select the "active" figure(s) and toggle emulation mode.
- Lets you download raw figure data backups through that same web UI.

No physical figurines or NFC readers are part of the finished device. Data acquisition (dumping real figures) is a one-time, separate, PC-side task done before firmware is built.

---

## 1. Prerequisites / Data Acquisition (do this first, outside the firmware project)

Goal: get raw dumps of every figure you own into flat binary files before writing any Rust.

- [ ] Acquire a cheap USB NFC reader for your PC (e.g. ACR122U or a PN532 USB dongle).
- [ ] Use existing community tooling (SkyReader-derived tools, or Dolphin emulator's built-in Skylanders portal/figure tooling) to dump each figure to a `.bin`/`.sky` file.
- [ ] Do the same for any Disney Infinity figures using available Infinity-focused community tools.
- [ ] Organize dumps into a folder structure, one file per figure, clearly named.
- [ ] Write down for each figure: display name, character ID / variant ID (if the tool reports it), and which game line it belongs to (Skylanders vs Infinity).
- [ ] Verify each dump by reading it back with the same PC tool before trusting it — a bad one-time dump means a bad figure forever once baked into firmware.

Definition of done: a folder of verified raw figure dumps + a small manifest (even just a text file) mapping filename → figure name → game line.

---

## 2. Toolchain & Environment Setup

- [ ] Install Rust via rustup; add target `thumbv8m.main-none-eabihf` (RP2350 core).
- [ ] Install `probe-rs-tools` for flashing/debugging (via SWD, if you have a debug probe) — or plan to use UF2 drag-and-drop as a fallback flashing method.
- [ ] Install `flip-link` (stack overflow protection, standard in embassy templates).
- [ ] Set up `defmt` + `defmt-rtt` + `panic-probe` for logging (standard embassy debugging stack).
- [ ] Start from an existing embassy + RP2350 + CYW43 template (search GitHub for an "embassy pico2w" starter) rather than from scratch, to inherit correct memory layout, boot2/image-def config, and clock setup.
- [ ] Confirm you can build and flash a trivial "blink the onboard LED" program before doing anything else. This validates the whole toolchain.

Definition of done: LED blinks. Toolchain confirmed working end-to-end.

---

## 3. Project Structure

Recommended crate layout (single binary crate to start; split into internal modules, not separate crates, to keep it simple):

    src/
      main.rs              - entry point, task spawning
      wifi.rs              - cyw43 init, embassy-net stack bring-up, AP or STA mode
      web/
        mod.rs             - picoserve (or chosen) HTTP server setup
        routes.rs           - route handlers (select figure, toggle mode, download backup, status)
        ui_html.rs          - embedded HTML/CSS/JS for the control page (as static str/bytes)
      usb/
        mod.rs              - shared USB device state machine, mode toggle logic
        skylanders.rs        - Skylanders portal descriptors + protocol handling
        infinity.rs          - Disney Infinity base descriptors + protocol handling
      figures/
        mod.rs               - figure data table/index, lookup by ID
        data.rs              - generated/const byte arrays for each figure dump (or included via include_bytes!)
      state.rs               - shared app state (active figure, active mode) using embassy sync primitives
      config.rs               - WiFi credentials, static config values

Definition of done: project compiles with empty/stub modules wired together; task skeleton runs.

---

## 4. Phase Breakdown

### Phase A — WiFi + Web Server Bring-Up (no USB yet)

- [ ] Bring up `cyw43` driver + `embassy-net` stack; connect to your home WiFi (station mode) or optionally run as its own access point (AP mode) so you don't depend on home WiFi being present — decide which and document the choice.
- [ ] Get a DHCP-assigned or static IP and confirm you can ping the Pico from your phone.
- [ ] Stand up a minimal async HTTP server (e.g. `picoserve`) serving a single static "Hello" page.
- [ ] Confirm you can load that page from your phone's browser.
- [ ] Add a `/status` JSON endpoint returning a hardcoded placeholder (e.g. current mode, current figure) to establish the request/response pattern you'll reuse.

Definition of done: phone browser loads a page served entirely by the Pico over WiFi.

### Phase B — Minimal USB HID Device (no protocol logic yet)

- [ ] Bring up `embassy-usb` with a bare-bones custom HID device: arbitrary VID/PID, minimal report descriptor, no real command handling.
- [ ] Plug into a PC (not the Wii yet) and confirm it enumerates (`lsusb` / Device Manager should show it).
- [ ] Confirm you can send/receive a raw HID report from a simple PC-side test script (Python + `hidapi` is the easiest path here).

Definition of done: PC recognizes the Pico as a generic HID device and a basic report round-trips.

### Phase C — Skylanders Protocol Implementation

- [ ] Implement the exact device descriptor: VID `0x1430`, PID `0x0150`, HID device, single interrupt-IN endpoint for status, control-transfer-based command channel (bmRequestType 0x21, bRequest 0x09) for commands, both 32 bytes, zero-padded.
- [ ] Implement the status packet the portal continuously sends (`S` — figure-arrived events with slot IDs 10/11/... etc).
- [ ] Implement command handling: `R` (activate/read), `A` (query), `Q` (read block), `W` (write block — can be a no-op/ack-only stub since you don't need writeback), `C` (LED color, can be ignored or just acked), `Z`.
- [ ] Wire this up to the `figures` module: when the web UI marks a figure "active," the USB task should emit the appropriate status packet as if that figure were just placed on the portal, and answer subsequent `Q` block-reads with that figure's stored data.
- [ ] Test against a PC-side Skylanders-aware tool first (e.g. Dolphin's Skylanders portal support, or SkyReader-based tooling) before risking the real Wii — this isolates protocol bugs from Wii-specific USB quirks.
- [ ] Once PC-side testing passes, test on the actual Wii with a real Skylanders game.

Definition of done: a Skylanders game running on the Wii sees your baked-in figure appear on the portal when selected from the web UI.

### Phase D — Web UI: Figure Selection + Backups

- [ ] Build the actual control page: list of baked-in figures (name, game line), a "make active" button/action per figure, and current status display.
- [ ] Wire the "make active" action to update shared state (`state.rs`) that the USB task reads to decide what to report to the Wii.
- [ ] Add a per-figure "download backup" link/button that streams the figure's raw stored bytes back as a file download (e.g. `Content-Disposition: attachment`) — this is just serving the same const byte array you already embedded, via HTTP instead of USB.
- [ ] Confirm from your phone: select a figure, verify it shows up in-game; download a backup, verify the file opens/matches the original dump byte-for-byte.

Definition of done: full loop works for Skylanders — select in web UI, appears on Wii, can back up the data via phone.

### Phase E — Disney Infinity Protocol Implementation

- [ ] Implement the Infinity base's device descriptor: VID `0x0e6f`, PID `0x0129`, HID device.
- [ ] Reverse-engineer/reference community docs for its command set — expect this to be less complete than Skylanders' docs, so budget time for USB traffic capture (Wireshark + USBPcap, or similar) against your own real base if you still have one, or against community writeups.
- [ ] Implement whatever subset of the protocol is needed for basic figure presence/read (full base features like multi-figure positions and RGB lighting are stretch goals, not required for MVP).
- [ ] Reuse the same `figures` data + shared-state plumbing from Phase C/D, just behind the Infinity descriptor/command set instead.
- [ ] Test on PC first if any Infinity-aware PC tooling exists; otherwise go straight to a real Infinity-compatible game on Wii, cautiously.

Definition of done: an Infinity game on the Wii sees a baked-in figure appear when selected from the web UI.

### Phase F — Mode Toggle (Skylanders ⇄ Infinity)

- [ ] Implement a soft USB disconnect/reconnect: on mode toggle from the web UI, call the disconnect function, swap which descriptor set + command handler is active, then reconnect — forcing the Wii to re-enumerate as the other device type.
- [ ] Add a toggle control to the web UI, wired to this logic.
- [ ] Test toggling with the Wii already running the relevant game vs. toggling before launching the game — document which works reliably, since consoles can behave differently mid-session vs. at boot.
- [ ] Add a "please unplug and replug if the console doesn't notice" fallback note in the UI, since soft re-enumeration isn't 100% guaranteed on every host.

Definition of done: you can flip a switch in the web UI and have the Wii recognize the device as the other portal type without a firmware reflash.

### Phase G — Figure Data Integration (bulk)

- [ ] Convert your full folder of verified dumps (from step 1) into Rust byte arrays — either via a small build script that generates a `data.rs` from the binary files, or manually with `include_bytes!` per file.
- [ ] Build the figure index/table (`figures/mod.rs`) mapping an internal ID → name → game line → byte slice.
- [ ] Regenerate and reflash firmware with the full set; re-test a handful of figures end-to-end (not just the one or two used during development).

Definition of done: your entire real-world figure collection is represented and selectable in the web UI.

### Phase H — Polish

- [ ] Persist last-selected figure/mode across power cycles if desired (small use of flash storage — optional, since you removed the runtime filesystem requirement, this can be a simple key-value write to a reserved flash page rather than a full filesystem).
- [ ] Improve web UI styling/usability for phone screens.
- [ ] Add basic error/status feedback in the UI (e.g. "USB not connected to console," "WiFi reconnecting").
- [ ] Write a short personal README covering: flashing instructions, WiFi setup, how to add a new figure (requires reflash), known quirks per game.
- [ ] Clean up logging (defmt levels) so normal operation isn't noisy, but debugging is still possible if something breaks later.

Definition of done: the device is comfortable to use day-to-day without needing to look at logs or re-derive how it works.

---

## 5. Key Dependencies (Cargo.toml, high level)

- `embassy-rp` (rp235xa feature) — RP2350 HAL
- `embassy-executor`, `embassy-time`, `embassy-sync` — async runtime + shared state primitives
- `cyw43`, `cyw43-pio` — WiFi/BT chip driver
- `embassy-net` — TCP/IP stack
- `embassy-usb` — USB device stack (custom HID)
- `picoserve` (or similar) — async HTTP server for `embassy-net`
- `defmt`, `defmt-rtt`, `panic-probe` — logging/debugging
- `static_cell` or similar — for setting up embassy singletons cleanly

---

## 6. Cross-Cutting Concerns / Risks to Track

- USB timing sensitivity: real consoles can be less forgiving than PC hosts of slow or malformed responses — always validate protocol changes against a PC tool before testing on the Wii.
- Soft re-enumeration reliability (Phase F) is the least proven part of this plan — treat it as a spike/experiment early rather than assuming it'll "just work."
- Infinity protocol documentation is thinner than Skylanders' — budget extra time/uncertainty for Phase E.
- Flash budget: figure dumps are small (~1–2KB each), so even a large personal collection should comfortably fit in the Pico 2 W's 4MB flash alongside firmware — but double check actual total size once all dumps are collected.
- Keep Wi-Fi credentials out of source control (use a local config file or `.env`-style approach ignored by git).

---

## 7. Suggested Order of Work for an LLM Pairing Session

1. Phase A (WiFi + web server) — fully independent, safe to build/test without any console involved.
2. Phase B (minimal USB HID) — independent, safe to build/test against a PC only.
3. Phase C (Skylanders protocol) — builds on B, test against PC tooling before Wii.
4. Phase D (web UI wiring) — builds on A + C.
5. Phase G (bulk figure data) can happen in parallel with C/D once the single-figure path works, using just 1–2 figures initially.
6. Phase E (Infinity protocol) — same pattern as C, once C/D are proven.
7. Phase F (mode toggle) — only after both protocol implementations independently work.
8. Phase H (polish) — last.

Each phase above is scoped to be a self-contained unit of work with a clear "definition of done," suitable for handing to an LLM one phase at a time with the relevant prior code as context.
