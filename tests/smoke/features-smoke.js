// Смоук-тест функций 0.2: заметки, полка, встроенные плагины, обновления, карточки.
(async () => {
  const inv = window.__TAURI_INTERNALS__.invoke;
  const out = [];
  const step = async (name, fn) => {
    try {
      const r = await fn();
      out.push(`OK   ${name}: ${JSON.stringify(r)?.slice(0, 200)}`);
    } catch (e) {
      out.push(`ERR  ${name}: ${JSON.stringify(e)}`);
    }
  };
  let noteId;
  await step("note create", async () => { const n = await inv("note_create", { text: "smoke-заметка", color: null }); noteId = n.id; return n.text; });
  await step("note update", () => inv("note_update", { id: noteId, text: null, color: "green", pinned: true }));
  await step("notes list", () => inv("notes_list").then((l) => l.map((n) => [n.text, n.color, n.pinned])));
  await step("note delete", () => inv("note_delete", { id: noteId }));
  await step("note empty (must fail)", () => inv("note_create", { text: "   ", color: null }));
  await step("shelf add", () => inv("shelf_add", { paths: ["C:\\Windows\\win.ini", "C:\\nonexistent.file"] }));
  await step("shelf list", () => inv("shelf_list").then((l) => l.map((i) => [i.name, i.exists, i.size])));
  await step("shelf clear", () => inv("shelf_command", { command: "clear", id: null, ids: null }));
  await step("builtin plugins", () => inv("plugins_list").then((l) => l.map((p) => [p.manifest.id, p.builtin, p.enabled])));
  await step("builtin uninstall (must fail)", () => inv("plugin_uninstall", { id: "builtin.pomodoro" }));
  await step("cards", () => inv("cards_list").then((c) => c.filter((x) => x.enabled).map((x) => x.id)));
  await step("update check", () => inv("update_check"));
  return out.join("\n");
})()
