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
<h2>Add Identity</h2>
<form id="identityForm">
<label>Name<input name="name" required placeholder="Character name"></label>
<label>Character ID<input name="character_id" inputmode="numeric" required placeholder="Numeric ID"></label>
<label>Variant ID<input name="variant_id" inputmode="numeric" placeholder="0"></label>
<label>Source Notes<input name="source" placeholder="Optional notes"></label>
<button class="primary" type="submit">Create Identity</button>
</form>
</section>

<section>
<h2>Create Instance</h2>
<form id="instanceForm">
<label>Identity<select name="identity_id" id="instanceIdentity"></select></label>
<label>Instance Name<input name="name" required placeholder="Save slot name"></label>
<button class="primary" type="submit">Create Fresh Instance</button>
</form>
</section>

<section>
<h2>Upload</h2>
<form id="uploadInstanceForm">
<label>Instance Name<input name="name" required placeholder="Imported instance name"></label>
<label>Parent Identity<select name="identity_id" id="uploadIdentity"><option value="">None</option></select></label>
<label>Instance Binary<input name="file" type="file" required></label>
<button type="submit">Upload Instance</button>
</form>
<form id="uploadBackupForm">
<label>Backup Name<input name="name" required placeholder="Backup name"></label>
<label>Source Notes<input name="source" placeholder="Optional notes"></label>
<label>Backup Binary<input name="file" type="file" required></label>
<button type="submit">Upload Backup</button>
</form>
</section>

<section>
<h2>Identities</h2>
<div id="identities" class="list"></div>
</section>

<section>
<h2>Instances</h2>
<div id="instances" class="list"></div>
</section>

<section>
<h2>Backups</h2>
<div id="backups" class="list"></div>
</section>

<section>
<h2>Log</h2>
<div id="message">Ready.</div>
</section>
</main>

<script>
let library = {identities:[], instances:[], backups:[], active_instance_id:null};

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
}

function renderStatus(status) {
  const storage = status.storage || {};
  $("status").innerHTML =
    `<div>Mode: ${status.mode || "unknown"}</div>` +
    `<div>Active instance: ${status.active_instance ?? "none"}</div>` +
    `<div>Records: ${storage.identities || 0} identities, ${storage.instances || 0} instances, ${storage.backups || 0} backups</div>` +
    `<div>Storage: ${storage.used_bytes || 0} / ${storage.capacity_bytes || 0} bytes</div>` +
    `<div>Corrupt records: ${storage.corrupt_records || 0}</div>`;
}

function renderLibrary() {
  fillIdentitySelect("instanceIdentity", false);
  fillIdentitySelect("uploadIdentity", true);
  renderIdentities();
  renderInstances();
  renderBackups();
}

function fillIdentitySelect(id, includeNone) {
  const select = $(id);
  select.innerHTML = includeNone ? `<option value="">None</option>` : "";
  for (const item of library.identities) {
    const option = document.createElement("option");
    option.value = item.id;
    option.textContent = `#${item.id} ${item.name}`;
    select.appendChild(option);
  }
}

function itemShell(title, meta, actions) {
  return `<div class="item"><strong>${title}</strong><div class="meta">${meta}</div><div class="actions">${actions}</div></div>`;
}

function renderIdentities() {
  $("identities").innerHTML = library.identities.map(item => itemShell(
    `#${item.id} ${item.name}`,
    `${item.game} character ${item.character_id}, variant ${item.variant_id ?? "none"}, ${item.format}`,
    `<button onclick="renameRecord('identity',${item.id})">Rename</button>` +
    `<button onclick="deleteRecord('identity',${item.id})">Delete</button>` +
    `<a href="/api/identity/${item.id}.json">JSON</a>`
  )).join("") || "<div class='meta'>No identities.</div>";
}

function renderInstances() {
  $("instances").innerHTML = library.instances.map(item => {
    const active = item.id === library.active_instance_id ? " active" : "";
    return itemShell(
      `#${item.id} ${item.name}${active}`,
      `${item.game}, ${item.image_len} bytes, crc32 ${item.crc32}, identity ${item.identity_id || "none"}`,
      `<button onclick="selectInstance(${item.id})">Select</button>` +
      `<button onclick="cloneInstance(${item.id})">Clone</button>` +
      `<button onclick="renameRecord('instance',${item.id})">Rename</button>` +
      `<button onclick="deleteRecord('instance',${item.id})">Delete</button>` +
      `<a href="/api/instance/${item.id}.bin">Download</a>`
    );
  }).join("") || "<div class='meta'>No instances.</div>";
}

function renderBackups() {
  $("backups").innerHTML = library.backups.map(item => itemShell(
    `#${item.id} ${item.name}`,
    `${item.game}, ${item.image_len} bytes, crc32 ${item.crc32}`,
    `<button onclick="renameRecord('backup',${item.id})">Rename</button>` +
    `<button onclick="deleteRecord('backup',${item.id})">Delete</button>` +
    `<a href="/api/backup/${item.id}.bin">Download</a>` +
    `<a href="/api/backup/${item.id}.json">JSON</a>`
  )).join("") || "<div class='meta'>No backups.</div>";
}

$("identityForm").addEventListener("submit", async event => {
  event.preventDefault();
  try {
    say(await api("/api/identity/create", {method:"POST", body: qs(event.target)}));
    event.target.reset();
    await refreshAll();
  } catch (error) { say(error.message); }
});

$("instanceForm").addEventListener("submit", async event => {
  event.preventDefault();
  try {
    say(await api("/api/instance/create", {method:"POST", body: qs(event.target)}));
    event.target.reset();
    await refreshAll();
  } catch (error) { say(error.message); }
});

$("uploadInstanceForm").addEventListener("submit", async event => {
  event.preventDefault();
  const form = event.target;
  const file = form.elements.file.files[0];
  const query = `name=${enc(form.elements.name.value)}&identity_id=${enc(form.elements.identity_id.value)}`;
  try {
    say(await api(`/api/instance/upload?${query}`, {method:"POST", body: await file.arrayBuffer()}));
    form.reset();
    await refreshAll();
  } catch (error) { say(error.message); }
});

$("uploadBackupForm").addEventListener("submit", async event => {
  event.preventDefault();
  const form = event.target;
  const file = form.elements.file.files[0];
  const query = `name=${enc(form.elements.name.value)}&source=${enc(form.elements.source.value)}`;
  try {
    say(await api(`/api/backup/upload?${query}`, {method:"POST", body: await file.arrayBuffer()}));
    form.reset();
    await refreshAll();
  } catch (error) { say(error.message); }
});

async function post(path, params = "") {
  const result = await api(path, {method:"POST", body: params});
  say(result);
  await refreshAll();
}

async function selectInstance(id) {
  await post("/api/instance/select", `id=${id}`);
}

async function clearActive() {
  await post("/api/instance/clear-active");
}

async function compactStorage() {
  if (confirm("Compact storage now?")) await post("/api/storage/compact");
}

async function cloneInstance(id) {
  const name = prompt("Clone name");
  if (name) await post("/api/instance/clone", `source_id=${id}&name=${enc(name)}`);
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
