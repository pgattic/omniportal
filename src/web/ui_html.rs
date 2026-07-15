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
button{background:#ececec}
button.primary{background:#1f6feb;color:#fff;border-color:#1f6feb}
.row{display:flex;gap:8px;flex-wrap:wrap;align-items:center}
.row>*{flex:1 1 160px}
.list{display:grid;gap:8px}
.item{border:1px solid #ddd;border-radius:4px;padding:8px;background:#fafafa}
.item strong{display:block}
.meta{font-size:12px;color:#555;word-break:break-word}
.actions{display:flex;gap:6px;flex-wrap:wrap;margin-top:8px}
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
<button onclick="clearActive()">Clear Active</button>
<button onclick="compactStorage()">Compact Storage</button>
</div>
</section>

<section>
<h2>Add to Collection</h2>
<form id="entityForm">
<div class="row">
<label>Type<select id="catalogKind">
<option value="">All types</option>
<option value="character">Characters</option>
<option value="item">Items</option>
<option value="level-piece">Level pieces</option>
<option value="trap">Traps</option>
<option value="vehicle">Vehicles</option>
<option value="creation-crystal">Creation crystals</option>
<option value="trophy">Trophies</option>
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
<h2>Upload</h2>
<form id="uploadEntityForm">
<label>Entity Name<input name="name" required placeholder="Imported entity name"></label>
<label>Entity Binary<input name="file" type="file" required></label>
<button type="submit">Upload Entity</button>
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
let library = {identities:[], entities:[], active_entity_id:null};
let catalog = [];
let catalogTotal = 0;
let catalogTimer = 0;

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
  await loadCatalog();
  renderStatus(status);
  renderLibrary();
}

function renderStatus(status) {
  const storage = status.storage || {};
  $("status").innerHTML =
    `<div>Mode: ${status.mode || "unknown"}</div>` +
    `<div>Active entity: ${status.active_entity ?? "none"}</div>` +
    `<div>Records: ${storage.entities || 0} entities</div>` +
    `<div>Storage: ${storage.used_bytes || 0} / ${storage.capacity_bytes || 0} bytes</div>` +
    `<div>Corrupt records: ${storage.corrupt_records || 0}</div>`;
}

async function loadCatalog() {
  const kind = $("catalogKind").value;
  const search = $("catalogSearch").value.trim();
  const loaded = await api(`/api/catalog?kind=${enc(kind)}&q=${enc(search)}&limit=80`);
  catalog = loaded.skylanders || [];
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
  renderEntities();
}

function itemShell(title, meta, actions) {
  return `<div class="item"><strong>${title}</strong><div class="meta">${meta}</div><div class="actions">${actions}</div></div>`;
}

function renderEntities() {
  const entities = [...(library.entities || [])].sort((left, right) => left.name.localeCompare(right.name));
  $("entities").innerHTML = entities.map(item => {
    const active = item.id === library.active_entity_id ? " active" : "";
    const download = `<a href="/api/entity/${item.id}.bin">Download</a>`;
    const clone = item.data_mode === "mutable-image"
      ? `<button onclick="cloneEntity(${item.id})">Clone</button>`
      : `<button onclick="cloneEntity(${item.id})">Create mutable copy</button>`;
    return itemShell(
      `#${item.id} ${item.name}${active}`,
      `${item.kind}, ${item.data_mode}, ${item.image_len} bytes, crc32 ${item.crc32}`,
      `<button onclick="selectEntity(${item.id})">Select</button>` +
      clone +
      `<button onclick="renameRecord('entity',${item.id})">Rename</button>` +
      `<button onclick="deleteRecord('entity',${item.id})">Delete</button>` +
      download
    );
  }).join("") || "<div class='meta'>No collection entities.</div>";
}

$("catalogKind").addEventListener("change", () => loadCatalog().catch(error => say(error.message)));
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
  const query = `name=${enc(form.elements.name.value)}`;
  try {
    say(await api(`/api/entity/upload?${query}`, {method:"POST", body: await file.arrayBuffer()}));
    form.reset();
    await refreshAll();
  } catch (error) { say(error.message); }
});

async function post(path, params = "") {
  const result = await api(path, {method:"POST", body: params});
  say(result);
  await refreshAll();
}

async function selectEntity(id) {
  await post("/api/entity/select", `id=${id}`);
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
