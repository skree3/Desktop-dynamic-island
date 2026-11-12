// Смоук-тест IPC живого приложения: вызывает все команды «только для чтения»
// и печатает результат. Запуск: см. tests/smoke/README.md (через CDP WebView2).
(async () => {
  const inv = window.__TAURI_INTERNALS__.invoke;
  const calls = [
    ["app_bootstrap", {}],
    ["settings_get", {}],
    ["cards_list", {}],
    ["notifications_state", {}],
    ["notifications_listener_status", {}],
    ["media_sessions", {}],
    ["sysmon_snapshot", {}],
    ["bluetooth_state", {}],
    ["quick_state", {}],
    ["volume_state", {}],
    ["brightness_state", {}],
    ["timers_list", {}],
    ["clipboard_list", { query: null, limit: 5 }],
    ["game_mode_state", {}],
    ["apps_search", { query: "tele", limit: 5 }],
    ["integrations_status", {}],
    ["custom_apps_list", {}],
    ["ai_services", {}],
    ["plugins_list", {}],
    ["profiles_list", {}],
    ["themes_list", {}],
    ["hotkeys_list", {}],
    ["logs_list", { limit: 5, category: null, level: null }],
    ["monitors_list", {}],
    ["perf_memory", {}],
    ["environment_info", {}],
    ["about_info", {}],
    // негативные проверки валидации
    ["power_execute", { action: "shutdown", token: "forged" }],
    ["settings_update", { section: "evil", value: {} }],
    ["open_url", { url: "javascript:alert(1)" }],
    ["plugin_call", { pluginId: "nope.plugin", method: "notify", params: {} }],
  ];
  const out = [];
  for (const [cmd, args] of calls) {
    const t = performance.now();
    try {
      const r = await inv(cmd, args);
      const s = JSON.stringify(r);
      out.push(`OK   ${cmd} (${Math.round(performance.now() - t)} ms): ${s.length > 160 ? s.slice(0, 160) + "…" : s}`);
    } catch (e) {
      out.push(`ERR  ${cmd}: ${JSON.stringify(e)}`);
    }
  }
  return out.join("\n");
})()
