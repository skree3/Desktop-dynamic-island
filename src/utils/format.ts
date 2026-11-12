// Форматирование значений для UI.

export function formatBytes(bytes: number | null | undefined, digits = 1): string {
  if (bytes == null || !isFinite(bytes)) return "—";
  const units = ["Б", "КБ", "МБ", "ГБ", "ТБ"];
  let v = bytes;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(i === 0 ? 0 : digits)} ${units[i]}`;
}

export function formatSpeed(bps: number | null | undefined): string {
  if (bps == null) return "—";
  return `${formatBytes(bps)}/с`;
}

/** 75_000 → "1:15", 3_725_000 → "1:02:05" */
export function formatClock(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const pad = (n: number) => String(n).padStart(2, "0");
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${m}:${pad(s)}`;
}

/** Секундомер с десятыми: 61_234 → "1:01.2" */
export function formatStopwatch(ms: number): string {
  const tenths = Math.floor((ms % 1000) / 100);
  return `${formatClock(ms)}.${tenths}`;
}

export function formatTime(ts: number): string {
  return new Date(ts).toLocaleTimeString("ru-RU", { hour: "2-digit", minute: "2-digit" });
}

export function formatDateTime(ts: number): string {
  return new Date(ts).toLocaleString("ru-RU", { day: "2-digit", month: "2-digit", hour: "2-digit", minute: "2-digit", second: "2-digit" });
}

export function formatRelative(ts: number, now = Date.now()): string {
  const diff = Math.round((now - ts) / 1000);
  if (diff < 45) return "только что";
  if (diff < 3600) return `${Math.round(diff / 60)} мин назад`;
  if (diff < 86400) return `${Math.round(diff / 3600)} ч назад`;
  return new Date(ts).toLocaleDateString("ru-RU", { day: "numeric", month: "short" });
}

export function formatPercent(v: number | null | undefined): string {
  if (v == null || !isFinite(v)) return "Недоступно";
  return `${Math.round(v)}%`;
}

export function formatTemp(v: number | null | undefined): string {
  if (v == null || !isFinite(v)) return "Недоступно";
  return `${Math.round(v)}°C`;
}

export function hexToRgba(color: string, alpha: number): string {
  const c = color.trim();
  if (c.startsWith("#")) {
    let hex = c.slice(1);
    if (hex.length === 3 || hex.length === 4) hex = hex.split("").map((x) => x + x).join("");
    const r = parseInt(hex.slice(0, 2), 16);
    const g = parseInt(hex.slice(2, 4), 16);
    const b = parseInt(hex.slice(4, 6), 16);
    const a = hex.length === 8 ? parseInt(hex.slice(6, 8), 16) / 255 : 1;
    return `rgba(${r}, ${g}, ${b}, ${(a * alpha).toFixed(3)})`;
  }
  const m = c.match(/^rgba?\(([^)]+)\)$/);
  if (m) {
    const parts = m[1].split(",").map((p) => p.trim());
    const a = parts[3] !== undefined ? parseFloat(parts[3]) : 1;
    return `rgba(${parts[0]}, ${parts[1]}, ${parts[2]}, ${(a * alpha).toFixed(3)})`;
  }
  return c;
}

/** Текущая позиция медиа с интерполяцией по времени. */
export function mediaPosition(positionMs: number | null, updatedAt: number, playing: boolean, durationMs: number | null, now = Date.now()): number {
  if (positionMs == null) return 0;
  const pos = playing ? positionMs + Math.max(0, now - updatedAt) : positionMs;
  return durationMs ? Math.min(pos, durationMs) : pos;
}

/** Разбор строки вида "5", "5:00", "1:30:00", "90s", "10m", "1h" в миллисекунды. */
export function parseDuration(input: string): number | null {
  const s = input.trim().toLowerCase().replace(",", ".");
  if (!s) return null;
  const unit = s.match(/^(\d+(?:\.\d+)?)\s*(с|s|сек|м|m|мин|min|ч|h)$/);
  if (unit) {
    const n = parseFloat(unit[1]);
    const u = unit[2];
    if (["с", "s", "сек"].includes(u)) return Math.round(n * 1000);
    if (["ч", "h"].includes(u)) return Math.round(n * 3_600_000);
    return Math.round(n * 60_000);
  }
  if (/^\d+(\.\d+)?$/.test(s)) return Math.round(parseFloat(s) * 60_000); // число = минуты
  const parts = s.split(":").map((p) => Number(p));
  if (parts.some((p) => !Number.isFinite(p) || p < 0)) return null;
  if (parts.length === 2) return (parts[0] * 60 + parts[1]) * 1000;
  if (parts.length === 3) return (parts[0] * 3600 + parts[1] * 60 + parts[2]) * 1000;
  return null;
}
