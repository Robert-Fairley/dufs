const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const test = require("node:test");
const vm = require("node:vm");

const script = fs.readFileSync(path.join(__dirname, "../assets/index.js"), "utf8");

function element() {
  const classes = new Set(["hidden"]);
  return {
    classList: {
      add: value => classes.add(value),
      remove: value => classes.delete(value),
      contains: value => classes.has(value),
    },
    children: [],
    events: {},
    setAttribute(name, value) { this[name] = value; },
    addEventListener(name, callback) { this.events[name] = callback; },
    append(...children) { this.children.push(...children); },
  };
}

function ui(filename, data) {
  const elements = {
    ".download": element(),
    ".media-preview": element(),
    ".not-editable": element(),
    ".save-btn": element(),
    ".move-file": element(),
    ".delete-file": element(),
  };
  const location = { href: `http://localhost/files/${filename}?view`, origin: "http://localhost" };
  const document = {
    readyState: "loading",
    addEventListener() {},
    querySelector: selector => elements[selector],
    createElement: tag => ({ ...element(), tagName: tag }),
  };
  const window = { location, events: {}, addEventListener(name, callback) { this.events[name] = callback; } };
  const context = vm.createContext({ document, location, window, URLSearchParams, URL });
  vm.runInContext(script, context);
  context.testEditor = element();
  vm.runInContext("$editor = testEditor", context);
  context.DATA = { kind: "View", allow_upload: false, allow_delete: false, ...data };
  return { context, elements };
}

test("theme preference is namespaced, persists, and can follow the system", () => {
  const values = new Map([["dufs:theme", "dark"]]);
  const storage = {
    getItem: key => values.has(key) ? values.get(key) : null,
    setItem: (key, value) => values.set(key, value),
  };
  const { context } = ui("", {});
  const select = { value: "system", events: {}, addEventListener(name, callback) { this.events[name] = callback; } };
  context.window.localStorage = storage;
  context.document.getElementById = () => select;
  context.document.documentElement = { dataset: {} };

  context.setupTheme();
  assert.equal(select.value, "dark");
  assert.equal(context.document.documentElement.dataset.theme, "dark");
  select.value = "light";
  select.events.change();
  assert.equal(values.get("dufs:theme"), "light");
  assert.equal(context.document.documentElement.dataset.theme, "light");
  select.value = "system";
  select.events.change();
  assert.equal(values.get("dufs:theme"), "system");
  assert.equal(context.document.documentElement.dataset.theme, "system");
});

test("media previews use native players even when detected as editable", async () => {
  for (const [filename, tag] of [["movie.MP4", "video"], ["song.OGG", "audio"]]) {
    const { context, elements } = ui(filename, { kind: "Edit", editable: true, allow_upload: true });
    await context.setupEditorPage();
    const preview = elements[".media-preview"];
    const [player, error] = preview.children;
    assert.equal(player.tagName, tag);
    assert.equal(player.controls, true);
    assert.equal(player.src, `http://localhost/files/${filename}`);
    assert.equal(elements[".download"].href, player.src);
    assert.equal(elements[".save-btn"].classList.contains("hidden"), true);
    player.events.error();
    assert.equal(error.classList.contains("hidden"), false);
  }
});

test("viewing text is read-only", async () => {
  const { context } = ui("notes.txt", { editable: true, allow_upload: true });
  context.fetch = async () => ({
    status: 200,
    headers: { get: () => "text/plain; charset=utf-8" },
    text: async () => "sample",
  });
  await context.setupEditorPage();
  assert.equal(context.testEditor.value, "sample");
  assert.equal(context.testEditor.readOnly, true);
});

test("binary and text-classified images render as images and report load errors", async () => {
  for (const [filename, editable] of [["photo.WEBP", false], ["photo.JPG", false], ["drawing.svg", true], ["photo.HEIC", false]]) {
    const { context, elements } = ui(filename, { editable });
    await context.setupEditorPage();
    const [image, error] = elements[".media-preview"].children;
    assert.equal(image.tagName, "img");
    assert.equal(image.src, `http://localhost/files/${filename}`);
    assert.equal(elements[".media-preview"].classList.contains("hidden"), false);
    assert.equal(elements[".not-editable"].classList.contains("hidden"), true);
    image.events.error();
    assert.equal(image.classList.contains("hidden"), true);
    assert.equal(error.classList.contains("hidden"), false);
  }
});

test("editable SVG files can still be edited as text", async () => {
  const { context, elements } = ui("drawing.svg", { kind: "Edit", editable: true, allow_upload: true });
  context.fetch = async () => ({
    status: 200,
    headers: { get: () => "image/svg+xml; charset=utf-8" },
    text: async () => "<svg></svg>",
  });
  await context.setupEditorPage();
  assert.equal(context.testEditor.value, "<svg></svg>");
  assert.equal(context.testEditor.readOnly, false);
  assert.equal(elements[".save-btn"].classList.contains("hidden"), false);
  assert.equal(elements[".media-preview"].classList.contains("hidden"), true);
});

test("authenticated preview downloads request and use a token", async () => {
  const { context, elements } = ui("movie.mp4", { user: "alice" });
  const requests = [];
  let downloaded;
  context.fetch = async url => {
    requests.push(url.toString());
    return { ok: true, text: async () => "download-token" };
  };
  context.document.querySelectorAll = () => [elements[".download"]];
  context.document.body = {
    appendChild(link) { downloaded = link; },
    removeChild() {},
  };
  const createElement = context.document.createElement;
  context.document.createElement = tag => tag === "a" ? { click() {} } : createElement(tag);
  await context.setupEditorPage();
  await elements[".download"].events.click({
    preventDefault() {},
    currentTarget: { getAttribute: () => elements[".download"].href },
  });
  assert.equal(new URL(requests[0]).searchParams.has("tokengen"), true);
  assert.equal(new URL(downloaded.href).searchParams.get("token"), "download-token");
  assert.equal(new URL(downloaded.href).pathname, "/files/movie.mp4");
});

test("listing links media to preview and retains direct downloads", () => {
  const { context } = ui("", { allow_archive: false });
  const rows = [];
  context.testTableBody = { insertAdjacentHTML: (_, html) => rows.push(html) };
  vm.runInContext("$pathsTableBody = testTableBody", context);
  for (const [name, path_type] of [["movie.MP4", "File"], ["notes.txt", "File"], ["music", "Dir"]]) {
    context.addPath({ name, path_type, size: 1, mtime: 0 }, rows.length, false, false);
  }
  assert.match(rows[0], /href="http:\/\/localhost\/files\/movie\.MP4\?view"/);
  assert.match(rows[0], /href="http:\/\/localhost\/files\/movie\.MP4" title="Download file" download/);
  assert.match(rows[0], /title="View file"[^>]*href="http:\/\/localhost\/files\/movie\.MP4\?view"/);
  assert.match(rows[0], /ph-bold ph-file-video/);
  assert.match(rows[1], /href="http:\/\/localhost\/files\/notes\.txt\?view" class="preview-link"/);
  assert.doesNotMatch(rows[2], /download-action/);
  assert.match(rows[2], /ph-bold ph-folder/);
});

test("preview links open a dialog and closing it unloads the file", () => {
  const { context } = ui("", { paths: [{ name: "my #song.mp3", path_type: "File" }] });
  const table = element();
  const frame = {
    contentWindow: {},
    removeAttribute(name) {
      if (name === "src") delete this.src;
    },
  };
  const title = {};
  const download = {};
  const dialog = element();
  dialog.querySelector = selector => ({
    ".preview-frame": frame,
    "#preview-title": title,
    ".preview-download": download,
  })[selector];
  dialog.showModal = () => { dialog.open = true; };
  dialog.close = () => { dialog.open = false; dialog.events.close(); };
  context.testTableBody = table;
  vm.runInContext("$pathsTableBody = testTableBody", context);
  context.document.querySelector = () => dialog;
  context.setupPreviewDialog();

  const link = { dataset: { index: "0" } };
  let prevented = false;
  const event = {
    button: 0,
    target: { closest: () => link },
    preventDefault() { prevented = true; },
  };
  table.events.click(event);
  assert.equal(prevented, true);
  assert.equal(dialog.open, true);
  assert.equal(title.textContent, "my #song.mp3");
  assert.equal(frame.src, "http://localhost/files/my%20%23song.mp3?view&embed");
  assert.equal(download.href, "http://localhost/files/my%20%23song.mp3");
  dialog.events.close();
  assert.equal(frame.src, undefined);
  table.events.click(event);
  context.window.events.message({
    source: {},
    data: "dufs:close-preview",
  });
  assert.equal(dialog.open, true);
  context.window.events.message({
    source: frame.contentWindow,
    data: "dufs:close-preview",
  });
  assert.equal(dialog.open, false);
  assert.equal(frame.src, undefined);

  prevented = false;
  table.events.click({ ...event, ctrlKey: true });
  assert.equal(prevented, false);
  assert.equal(frame.src, undefined);
});
