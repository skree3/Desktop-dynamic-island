// Помодоро: состояние хранится в storage, поэтому таймер переживает
// сворачивание панели (время считается от момента старта).
const $ = (id) => document.getElementById(id);
let cfg = { work: 25, rest: 5, showInCompact: true };
let st = { phase: "work", running: false, endsAt: 0, left: 25 * 60 * 1000, done: 0, day: "" };
let tick = null;
let lastActivity = 0;

const today = () => new Date().toISOString().slice(0, 10);
const len = (phase) => (phase === "work" ? cfg.work : cfg.rest) * 60 * 1000;
const fmt = (ms) => {
  const s = Math.max(0, Math.round(ms / 1000));
  return `${String(Math.floor(s / 60)).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`;
};
const save = () => island.storage.set("state", st).catch(() => {});
const remaining = () => (st.running ? st.endsAt - Date.now() : st.left);

function render() {
  const left = remaining();
  $("time").textContent = fmt(left);
  $("phase").textContent = st.phase === "work" ? (st.running ? "Фокус" : "Работа") : st.running ? "Отдых" : "Перерыв";
  $("phase").className = `phase ${st.phase}`;
  $("toggle").textContent = st.running ? "Пауза" : "Старт";
  $("stats").textContent = `Сегодня завершено: ${st.day === today() ? st.done : 0}`;
  if (cfg.showInCompact && st.running && Date.now() - lastActivity > 15000) {
    lastActivity = Date.now();
    island.activity
      .set({ title: st.phase === "work" ? "Фокус" : "Отдых", subtitle: `осталось ${Math.ceil(left / 60000)} мин`, progress: 1 - left / len(st.phase) })
      .catch(() => {});
  }
}

async function finishPhase() {
  const wasWork = st.phase === "work";
  if (wasWork) {
    if (st.day !== today()) { st.day = today(); st.done = 0; }
    st.done += 1;
  }
  st.phase = wasWork ? "rest" : "work";
  st.running = false;
  st.left = len(st.phase);
  await save();
  island.activity.clear().catch(() => {});
  await island.notify({
    title: wasWork ? "Время отдохнуть" : "Перерыв окончен",
    body: wasWork ? `Отличная работа! Перерыв ${cfg.rest} мин.` : `Следующий подход — ${cfg.work} мин.`,
    priority: "high",
    actions: [{ id: "start", label: "Начать" }],
  });
  render();
}

function loop() {
  clearInterval(tick);
  tick = setInterval(() => {
    if (st.running && remaining() <= 0) finishPhase();
    render();
  }, 500);
}

$("toggle").onclick = async () => {
  if (st.running) {
    st.left = remaining();
    st.running = false;
    island.activity.clear().catch(() => {});
  } else {
    st.endsAt = Date.now() + st.left;
    st.running = true;
    lastActivity = 0;
  }
  await save();
  render();
};
$("skip").onclick = () => finishPhase();
$("reset").onclick = async () => {
  st = { ...st, phase: "work", running: false, left: len("work") };
  island.activity.clear().catch(() => {});
  await save();
  render();
};

island.on("notificationAction", (d) => {
  if (d.action === "start" && !st.running) $("toggle").click();
});
island.on("settingsChanged", async () => {
  cfg = { ...cfg, ...(await island.settings.get()) };
  if (!st.running) st.left = len(st.phase);
  render();
});

(async () => {
  cfg = { ...cfg, ...(await island.settings.get()) };
  const saved = await island.storage.get("state");
  if (saved) st = { ...st, ...saved };
  else st.left = len("work");
  render();
  loop();
})();
