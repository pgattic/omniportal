pub const INDEX_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>OmniPortal</title>
<style>
body{font-family:system-ui,sans-serif;margin:0;background:#f5f5f5;color:#1c1c1c}
main{max-width:880px;margin:0 auto;padding:16px}
h1{font-size:24px;margin:0 0 12px}
h2{font-size:18px;margin:0 0 10px}
section{background:#fff;border:1px solid #d8d8d8;border-radius:6px;margin:12px 0;padding:12px}
form{display:grid;gap:8px;margin:8px 0}
label{display:grid;gap:4px;font-size:13px}
input,select,button{font:inherit;padding:8px;border:1px solid #bbb;border-radius:4px;background:#fff}
input,select{box-sizing:border-box;min-width:0;max-width:100%}
label>input,label>select{width:100%}
button{background:#ececec}
button.primary{background:#1f6feb;color:#fff;border-color:#1f6feb}
.row{display:flex;gap:8px;flex-wrap:wrap;align-items:center}
.row>*{flex:1 1 160px}
.list{display:grid;gap:8px}
.item{border:1px solid #ddd;border-radius:4px;padding:8px;background:#fafafa}
.item strong{display:block}
.meta{font-size:12px;color:#555;word-break:break-word}
.actions{display:flex;gap:6px;flex-wrap:wrap;margin-top:8px}
.actions select{width:auto;min-width:120px;max-width:100%}
.slots{display:grid;grid-template-columns:repeat(auto-fit,minmax(180px,1fr));gap:8px}
.slot{border:1px solid #ddd;border-radius:4px;padding:8px;background:#fafafa;display:grid;gap:8px}
.slot.empty{background:#fff}
.slot-title{font-weight:700}
.portal-add{align-content:start}
.portal-add button{font-size:20px;line-height:1;padding:8px 12px}
#message{white-space:pre-wrap;font-family:ui-monospace,monospace;font-size:12px;background:#111;color:#eee;padding:8px;border-radius:4px}
</style>
</head>
<body>
<main>
<h1>OmniPortal</h1>

<section>
<h2>Status</h2>
<div id="status">Loading...</div>
<div class="row">
<button onclick="refreshAll()">Refresh</button>
<button onclick="compactStorage()">Compact Storage</button>
</div>
</section>

<section>
<h2>Portal of Power</h2>
<div id="slots" class="slots"></div>
<div class="actions">
<button onclick="clearActive()">Clear Portal</button>
</div>
</section>

<section>
<h2>Add to Collection</h2>
<form id="entityForm">
<div class="row">
<label>Game<select name="game" id="catalogGame">
<option value="skylanders">Skylanders</option>
<option value="infinity">Disney Infinity</option>
</select></label>
<label>Type<select id="catalogKind">
<option value="">All types</option>
<option value="character">Characters</option>
<option value="item">Items</option>
<option value="level-piece">Level pieces</option>
<option value="trap">Traps</option>
<option value="vehicle">Vehicles</option>
<option value="creation-crystal">Creation crystals</option>
<option value="trophy">Trophies</option>
<option value="power-disc">Power discs</option>
</select></label>
<label>Search<input id="catalogSearch" placeholder="Filter catalog"></label>
</div>
<label>Figure<select name="catalog_index" id="catalogSelect"></select></label>
<label>Entity Name<input name="name" required placeholder="Name"></label>
<button class="primary" type="submit">Add Entity</button>
<div class="meta" id="catalogCount"></div>
</form>
</section>

<section>
<h2>Import</h2>
<form id="uploadEntityForm">
<label>Game<select name="game">
<option value="skylanders">Skylanders</option>
<option value="infinity">Disney Infinity</option>
</select></label>
<label>Entity Name<input name="name" required placeholder="Imported entity name"></label>
<label>Entity Binary<input name="file" type="file" required></label>
<button type="submit">Import Entity</button>
</form>
</section>

<section>
<h2>Collection</h2>
<div id="entities" class="list"></div>
</section>

<section>
<h2>Log</h2>
<div id="message">Ready.</div>
</section>
</main>

<script>
let library = {identities:[], entities:[], active_entity_id:null, active_slots:[]};
let catalog = [];
let catalogTotal = 0;
let catalogTimer = 0;
let currentMode = "skylanders";
const skylandersPortalSlotCount = 8;
const infinityPortalSlots = [
  {slot: 0, label: "Player 1", accepts: item => item.game === "infinity" && (item.kind === "character" || item.kind === "unknown")},
  {slot: 1, label: "Player 2", accepts: item => item.game === "infinity" && (item.kind === "character" || item.kind === "unknown")},
  {slot: 2, label: "Item / Disc", accepts: item => item.game === "infinity" && item.kind !== "character" && item.kind !== "unknown"}
];

const $ = id => document.getElementById(id);
const enc = value => encodeURIComponent(value == null ? "" : value);
const qs = form => new URLSearchParams(new FormData(form)).toString();

async function api(path, options) {
  const res = await fetch(path, options);
  const text = await res.text();
  if (!res.ok) throw new Error(text || res.statusText);
  try { return JSON.parse(text); } catch (_) { return text; }
}

function say(value) {
  $("message").textContent = typeof value === "string" ? value : JSON.stringify(value, null, 2);
}

async function refreshAll() {
  const status = await api("/status");
  library = await api("/api/library");
  renderStatus(status);
  renderLibrary();
  await loadCatalog();
}

function renderStatus(status) {
  currentMode = status.mode || "skylanders";
  const storage = status.storage || {};
  const slots = status.active_slots || [];
  const used = storage.used_bytes || 0;
  const capacity = storage.capacity_bytes || 0;
  $("status").innerHTML =
    `<div>Mode: ${status.mode || "unknown"}</div>` +
    `<div>Figures on portal: ${slots.length}</div>` +
    `<div>Records: ${storage.entities || 0} entities</div>` +
    `<div>Storage: ${used} / ${capacity} bytes (${storagePercent(used, capacity)})</div>` +
    `<div>Corrupt records: ${storage.corrupt_records || 0}</div>`;
}

async function loadCatalog() {
  const game = $("catalogGame").value;
  const kind = $("catalogKind").value;
  const search = $("catalogSearch").value.trim();
  const loaded = await api(`/api/catalog?game=${enc(game)}&kind=${enc(kind)}&q=${enc(search)}&limit=30`);
  catalog = loaded.figures || loaded.skylanders || [];
  catalogTotal = loaded.total || catalog.length;
  renderCatalog();
}

function renderCatalog() {
  const select = $("catalogSelect");
  select.innerHTML = "";
  for (const item of catalog) {
    const option = document.createElement("option");
    option.value = item.index;
    option.textContent = `${item.name} (${item.kind}, ${item.series})`;
    select.appendChild(option);
  }
  $("catalogCount").textContent = `${catalogTotal} matching entries${catalogTotal > catalog.length ? `; showing first ${catalog.length}` : ""}.`;
}

function renderLibrary() {
  renderSlots();
  renderEntities();
}

function itemShell(title, meta, actions) {
  return `<div class="item"><strong>${title}</strong><div class="meta">${meta}</div><div class="actions">${actions}</div></div>`;
}

function renderEntities() {
  const entities = [...(library.entities || [])].sort((left, right) => left.name.localeCompare(right.name));
  const activeSlots = library.active_slots || [];
  $("entities").innerHTML = entities.map(item => {
    const slots = activeSlots
      .filter(slot => slot.entity_id === item.id)
      .map(slot => Number(slot.slot) + 1);
    const active = slots.length ? " (active)" : "";
    const download = `<a href="/api/entity/${item.id}.bin">Export</a>`;
    const clone = `<button onclick="cloneEntity(${item.id})">Clone</button>`;
    const place_remove = slots.length
      ? slots.map(slot => `<button onclick="removeSlot(${slot - 1})">Remove from Portal</button>`).join("")
      : `<button onclick="placeEntityFirstAvailable(${item.id})">Place on Portal</button>`;
    return itemShell(
      `#${item.id} ${escapeHtml(item.name)}${active}`,
      entityMeta(item),
      place_remove +
      clone +
      `<button onclick="renameRecord('entity',${item.id})">Rename</button>` +
      `<button onclick="deleteRecord('entity',${item.id})">Delete</button>` +
      download
    );
  }).join("") || "<div class='meta'>No collection entities.</div>";
}

function renderSlots() {
  const entities = library.entities || [];
  const activeSlots = library.active_slots || [];
  const sortedEntities = [...entities].sort((left, right) => left.name.localeCompare(right.name));
  if (currentMode === "infinity") {
    $("slots").innerHTML = infinityPortalSlots.map(portal => {
      const active = activeSlots.find(item => Number(item.slot) === portal.slot);
      const entity = active && entities.find(item => item.id === active.entity_id);
      if (entity) {
        return `<div class="slot">` +
          `<div class="slot-title">${portal.label}</div>` +
          `<strong>${escapeHtml(entity.name)}</strong>` +
          `<div class="meta">${entityMeta(entity)}</div>` +
          `<div class="actions"><button onclick="removeSlot(${portal.slot})">Remove</button></div>` +
          `</div>`;
      }
      const options = entityOptions(sortedEntities.filter(item =>
        portal.accepts(item) && !activeSlots.some(active => active.entity_id === item.id)
      ), null);
      return `<div class="slot empty portal-add">` +
        `<div class="slot-title">${portal.label}</div>` +
        `<div class="actions">` +
        `<select id="portalAddSelect${portal.slot}">${options}</select>` +
        `<button title="Place on Portal" onclick="placePortalAddSelect(${portal.slot})" ${options ? "" : "disabled"}>+</button>` +
        `</div></div>`;
    }).join("");
    return;
  }

  const visibleActiveSlots = activeSlots.filter(active => Number(active.slot) < skylandersPortalSlotCount);
  const activeCards = [...visibleActiveSlots]
    .sort((left, right) => Number(left.slot) - Number(right.slot))
    .map(active => {
      const portalIndex = Number(active.slot);
      const entity = entities.find(item => item.id === active.entity_id);
      if (!entity) return "";
      return `<div class="slot">` +
        `<div class="slot-title">${escapeHtml(entity.name)}</div>` +
        `<div class="meta">${entityMeta(entity)}</div>` +
        `<div class="actions">` +
        `<button onclick="removeSlot(${portalIndex})">Remove</button>` +
        `</div></div>`;
    })
    .join("");
  const availableEntities = sortedEntities.filter(item => item.game === "skylanders" && !activeSlots.some(active => active.entity_id === item.id));
  const addCard = firstEmptySlot(activeSlots) == null
    ? `<div class="slot empty"><div class="slot-title">Portal Full</div><div class="meta">Remove a figure before adding another.</div></div>`
    : `<div class="slot empty portal-add">` +
      `<div class="slot-title">Add Figure</div>` +
      `<div class="actions">` +
      `<select id="portalAddSelect">${entityOptions(availableEntities, null)}</select>` +
      `<button title="Place on Portal" onclick="placePortalAddSelect()" ${availableEntities.length ? "" : "disabled"}>+</button>` +
      `</div></div>`;
  $("slots").innerHTML = (activeCards || "<div class='meta'>Portal is empty.</div>") + addCard;
}

function entityOptions(entities, selectedId) {
  return entities.map(item => {
    const selected = item.id === selectedId ? " selected" : "";
    return `<option value="${item.id}"${selected}>#${item.id} ${escapeHtml(item.name)}</option>`;
  }).join("");
}

function firstEmptySlot(activeSlots) {
  for (let slot = 0; slot < skylandersPortalSlotCount; slot++) {
    if (!activeSlots.some(item => Number(item.slot) === slot)) return slot;
  }
  return null;
}

function firstAvailableSlotForEntity(entity) {
  const activeSlots = library.active_slots || [];
  if (currentMode === "infinity") {
    if (entity.game !== "infinity") return null;
    const portal = infinityPortalSlots.find(portal =>
      portal.accepts(entity) && !activeSlots.some(item => Number(item.slot) === portal.slot)
    );
    return portal ? portal.slot : null;
  }
  if (entity.game !== "skylanders") return null;
  return firstEmptySlot(activeSlots);
}

function entityMeta(item) {
  const figure = item.figure || `ID ${item.character_id}`;
  return `${escapeHtml(figure)}, ${item.kind}, ${item.game}`;
}

function storagePercent(used, capacity) {
  if (!capacity) return "0%";
  return `${((used / capacity) * 100).toFixed(1)}%`;
}

async function fileHex(file) {
  const bytes = new Uint8Array(await file.arrayBuffer());
  let out = "";
  for (const byte of bytes) out += byte.toString(16).padStart(2, "0");
  return out;
}

function escapeHtml(value) {
  return String(value)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

$("catalogKind").addEventListener("change", () => loadCatalog().catch(error => say(error.message)));
$("catalogGame").addEventListener("change", () => loadCatalog().catch(error => say(error.message)));
$("catalogSearch").addEventListener("input", () => {
  clearTimeout(catalogTimer);
  catalogTimer = setTimeout(() => loadCatalog().catch(error => say(error.message)), 250);
});

$("entityForm").addEventListener("submit", async event => {
  event.preventDefault();
  try {
    say(await api("/api/entity/create-from-catalog", {method:"POST", body: qs(event.target)}));
    event.target.reset();
    await loadCatalog();
    await refreshAll();
  } catch (error) { say(error.message); }
});

$("uploadEntityForm").addEventListener("submit", async event => {
  event.preventDefault();
  const form = event.target;
  const file = form.elements.file.files[0];
  if (!file) {
    say("Choose a binary file to import.");
    return;
  }
  if (form.elements.game.value === "infinity" && file.size !== 320) {
    say(`Disney Infinity imports must be exactly 320 bytes; selected file is ${file.size} bytes.`);
    return;
  }
  try {
    say(`Uploading ${file.name} (${file.size} bytes)...`);
    const imageHex = await fileHex(file);
    const body = `game=${enc(form.elements.game.value)}&name=${enc(form.elements.name.value)}&image_hex=${imageHex}`;
    say(await api("/api/entity/upload", {method:"POST", body}));
    form.reset();
    await refreshAll();
  } catch (error) { say(error.message); }
});

async function post(path, params = "") {
  const result = await api(path, {method:"POST", body: params});
  say(result);
  await refreshAll();
}

async function placeEntityInSlot(id, slot) {
  const index = Number(slot);
  const limit = currentMode === "infinity" ? infinityPortalSlots.length : skylandersPortalSlotCount;
  if (!Number.isInteger(index) || index < 0 || index >= limit) {
    say("portal target is invalid");
    return;
  }
  await post("/api/entity/select", `id=${id}&slot=${index}`);
}

async function placeEntityFirstAvailable(id) {
  const entity = (library.entities || []).find(item => item.id === id);
  const slot = entity ? firstAvailableSlotForEntity(entity) : null;
  if (slot == null) {
    say("no compatible portal position is available");
    return;
  }
  await placeEntityInSlot(id, slot);
}

async function placePortalAddSelect(slot = null) {
  const select = slot == null ? $("portalAddSelect") : $(`portalAddSelect${slot}`);
  const id = Number(select.value);
  if (!Number.isInteger(id)) return;
  if (slot == null) await placeEntityFirstAvailable(id);
  else await placeEntityInSlot(id, slot);
}

async function removeSlot(slot) {
  await post("/api/entity/clear-active", `slot=${slot}`);
}

async function clearActive() {
  await post("/api/entity/clear-active");
}

async function compactStorage() {
  if (confirm("Compact storage now?")) await post("/api/storage/compact");
}

async function cloneEntity(id) {
  const name = prompt("Clone name");
  if (name) await post("/api/entity/clone", `source_id=${id}&name=${enc(name)}`);
}

async function renameRecord(kind, id) {
  const name = prompt(`New ${kind} name`);
  if (name) await post(`/api/${kind}/rename`, `id=${id}&name=${enc(name)}`);
}

async function deleteRecord(kind, id) {
  if (confirm(`Delete ${kind} #${id}?`)) await post(`/api/${kind}/delete`, `id=${id}`);
}

refreshAll().catch(error => say(error.message));
</script>
</body>
</html>
"#;
