// Привычки: список хранится в storage плагина.
const $ = (id) => document.getElementById(id);
let habits = [];

const dayKey = (offset = 0) => {
  const d = new Date();
  d.setDate(d.getDate() - offset);
  return d.toISOString().slice(0, 10);
};
const save = () => island.storage.set("habits", habits).catch(() => {});

function streak(h) {
  let n = 0;
  for (let i = h.done.includes(dayKey(0)) ? 0 : 1; h.done.includes(dayKey(i)); i++) n++;
  return n;
}

function render() {
  const list = $("list");
  list.innerHTML = "";
  if (!habits.length) {
    list.innerHTML = '<div class="muted" style="text-align:center;padding:12px">Добавьте первую привычку</div>';
  }
  for (const h of habits) {
    const row = document.createElement("div");
    row.className = "habit";
    const doneToday = h.done.includes(dayKey(0));
    const check = document.createElement("button");
    check.className = `check ${doneToday ? "on" : ""}`;
    check.textContent = doneToday ? "✓" : "";
    check.title = doneToday ? "Снять отметку" : "Отметить на сегодня";
    check.onclick = () => {
      h.done = doneToday ? h.done.filter((d) => d !== dayKey(0)) : [...h.done, dayKey(0)].slice(-60);
      save();
      render();
    };
    const info = document.createElement("div");
    info.className = "grow";
    const title = document.createElement("div");
    title.textContent = h.name;
    const meta = document.createElement("div");
    meta.className = "muted";
    const s = streak(h);
    meta.textContent = s ? `Серия: ${s} дн.` : "Серии пока нет";
    info.append(title, meta);
    const days = document.createElement("div");
    days.className = "days";
    for (let i = 6; i >= 0; i--) {
      const dot = document.createElement("i");
      if (h.done.includes(dayKey(i))) dot.className = "on";
      days.append(dot);
    }
    const del = document.createElement("button");
    del.className = "del";
    del.textContent = "✕";
    del.title = "Удалить";
    del.onclick = () => {
      habits = habits.filter((x) => x !== h);
      save();
      render();
    };
    row.append(check, info, days, del);
    list.append(row);
  }
}

$("form").onsubmit = (e) => {
  e.preventDefault();
  const name = $("name").value.trim();
  if (!name || habits.length >= 20) return;
  habits.push({ name, done: [] });
  $("name").value = "";
  save();
  render();
};

(async () => {
  habits = (await island.storage.get("habits")) || [];
  render();
})();
