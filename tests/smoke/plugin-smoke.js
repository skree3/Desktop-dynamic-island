// Смоук-тест системы плагинов: установка → отказ без подтверждения прав →
// включение с подтверждением → вызовы API → отказ по неразрешённому праву.
(async () => {
  const inv = window.__TAURI_INTERNALS__.invoke;
  const out = [];
  const step = async (name, fn) => {
    try {
      const r = await fn();
      out.push(`OK   ${name}: ${JSON.stringify(r)?.slice(0, 180)}`);
    } catch (e) {
      out.push(`ERR  ${name}: ${JSON.stringify(e)}`);
    }
  };
  const id = "dev.example.system-pulse";
  await step("install", () => inv("plugin_install", { path: "D:\\Dynamic Island\\desktop-island\\examples\\plugins\\system-pulse" }).then((r) => [r.plugin.manifest.version, r.plugin.pendingPermissions]));
  await step("enable without approval (must fail)", () => inv("plugin_set_enabled", { id, enabled: true, approvePermissions: false }));
  await step("enable with approval", () => inv("plugin_set_enabled", { id, enabled: true, approvePermissions: true }).then((p) => [p.enabled, p.granted]));
  await step("api app.info", () => inv("plugin_call", { pluginId: id, method: "app.info", params: {} }));
  await step("api storage.set", () => inv("plugin_call", { pluginId: id, method: "storage.set", params: { key: "k", value: 42 } }));
  await step("api storage.get", () => inv("plugin_call", { pluginId: id, method: "storage.get", params: { key: "k" } }));
  await step("api system.snapshot", () => inv("plugin_call", { pluginId: id, method: "system.snapshot", params: {} }).then((s) => ({ cpu: s.cpu })));
  await step("api media.control (not granted → must fail)", () => inv("plugin_call", { pluginId: id, method: "media.control", params: { action: "next" } }));
  await step("setting invalid select (must fail)", () => inv("plugin_setting_set", { id, key: "interval", value: "1 час" }));
  await step("setting valid", () => inv("plugin_setting_set", { id, key: "interval", value: "2 с" }));
  await step("cards include plugin", () => inv("cards_list").then((c) => c.filter((x) => x.pluginId).map((x) => x.id)));
  return out.join("\n");
})()
