// Пример плагина Desktop Island. Весь доступ — через window.island (Plugin SDK).
const $ = (id) => document.getElementById(id);
let timer = null;
let settings = {};

async function refresh() {
  try {
    const s = await island.system.snapshot();
    if (!s) return;
    const cpu = Math.round(s.cpu);
    const ram = s.ramTotal ? Math.round((s.ramUsed / s.ramTotal) * 100) : null;
    $("cpu").textContent = `${cpu}%`;
    $("ram").textContent = ram == null ? "—" : `${ram}%`;
    if (settings.showActivity) await island.activity.set({ title: `CPU ${cpu}%`, subtitle: "System Pulse", progress: cpu / 100 });
  } catch (e) {
    $("status").textContent = `Ошибка: ${e.message}`;
  }
}

async function loadSettings() {
  settings = await island.settings.get();
  const sec = parseInt(settings.interval, 10) || 5;
  clearInterval(timer);
  timer = setInterval(refresh, sec * 1000);
  if (!settings.showActivity) island.activity.clear().catch(() => {});
  refresh();
}

$("hello").onclick = async () => {
  await island.notify({
    title: settings.greeting || "Привет!",
    body: "Это уведомление отправил плагин через Event Manager.",
    actions: [{ id: "ok", label: "Отлично" }],
  });
};

$("count").onclick = async () => {
  const n = ((await island.storage.get("clicks")) || 0) + 1;
  await island.storage.set("clicks", n);
  $("clicks").textContent = n;
};

island.on("settingsChanged", loadSettings);
island.on("notificationAction", (d) => ($("status").textContent = `Нажато действие «${d.action}»`));

(async () => {
  const info = await island.info();
  $("status").textContent = `Plugin API v${info.apiVersion} · Desktop Island ${info.appVersion} · ${info.os}`;
  $("clicks").textContent = (await island.storage.get("clicks")) || 0;
  await loadSettings();
})();
