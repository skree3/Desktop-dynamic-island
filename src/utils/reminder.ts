// Напоминания словами: «через 20 мин», «через 2 часа», «в 18:30», «завтра 9:00»,
// «завтра в 9», «в 7 вечера», «послезавтра в 10». Возвращает время и текст
// без слов про время. Время в прошлом сегодня переносится на завтра.

export interface ParsedReminder {
  at: number;
  text: string;
}

const UNITS: [RegExp, number][] = [
  [/^(мин|минут[уы]?|м)$/, 60_000],
  [/^(ч|час|часа|часов)$/, 3_600_000],
  [/^(сек|секунд[уы]?|с)$/, 1000],
  [/^(дн|день|дня|дней)$/, 86_400_000],
];

function clean(text: string): string {
  return text.replace(/\s{2,}/g, " ").replace(/^[\s,.:;—-]+|[\s,.:;—-]+$/g, "");
}

export function parseReminder(input: string, now: Date = new Date()): ParsedReminder | null {
  const src = input.trim();
  if (!src) return null;
  const lower = src.toLowerCase();

  // «через полчаса», «через час», «через 20 мин».
  let m = lower.match(/через\s+(полчаса|час|минуту|(\d{1,3})\s*([а-яё]+))/);
  if (m) {
    let ms = 0;
    if (m[1] === "полчаса") ms = 30 * 60_000;
    else if (m[1] === "час") ms = 3_600_000;
    else if (m[1] === "минуту") ms = 60_000;
    else {
      const unit = UNITS.find(([re]) => re.test(m![3]));
      if (unit) ms = Number(m[2]) * unit[1];
    }
    if (ms > 0) return { at: now.getTime() + ms, text: clean(src.slice(0, m.index) + src.slice(m.index! + m[0].length)) };
  }

  // «[завтра|послезавтра] [в] 18:30 | 9 [утра|вечера|дня|ночи]».
  m = lower.match(/(послезавтра|завтра|сегодня)?\s*(?:в\s*)?(\d{1,2})(?:[:.](\d{2}))?\s*(утра|дня|вечера|ночи)?/);
  while (m) {
    const hasDay = !!m[1];
    const hasMinutes = m[3] !== undefined;
    const part = m[4];
    // Одиночное число без «в», «завтра» и времени суток — это не время («купить 3 яблока»).
    const explicitAt = /(^|\s)в\s*\d/.test(m[0]) || hasDay || hasMinutes || !!part;
    let h = Number(m[2]);
    const min = hasMinutes ? Number(m[3]) : 0;
    if (explicitAt && h < 24 && min < 60) {
      if ((part === "вечера" || part === "дня") && h < 12) h += 12;
      if (part === "ночи" && h === 12) h = 0;
      const d = new Date(now);
      d.setSeconds(0, 0);
      d.setHours(h, min);
      if (m[1] === "завтра") d.setDate(d.getDate() + 1);
      else if (m[1] === "послезавтра") d.setDate(d.getDate() + 2);
      else if (d.getTime() <= now.getTime()) d.setDate(d.getDate() + 1);
      return { at: d.getTime(), text: clean(src.slice(0, m.index) + src.slice(m.index! + m[0].length)) };
    }
    const rest = lower.slice(m.index! + Math.max(1, m[0].length));
    const next = rest.match(/(послезавтра|завтра|сегодня)?\s*(?:в\s*)?(\d{1,2})(?:[:.](\d{2}))?\s*(утра|дня|вечера|ночи)?/);
    if (!next) break;
    next.index = next.index! + (lower.length - rest.length);
    m = next;
  }
  return null;
}

/** Короткая подпись времени напоминания: «18:30», «завтра 9:00», «пт 20:00». */
export function reminderLabel(at: number, now: Date = new Date()): string {
  const d = new Date(at);
  const hm = `${d.getHours()}:${String(d.getMinutes()).padStart(2, "0")}`;
  const day = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const diff = Math.round((day(d) - day(now)) / 86_400_000);
  if (diff === 0) return hm;
  if (diff === 1) return `завтра ${hm}`;
  if (diff > 1 && diff < 7) return `${["вс", "пн", "вт", "ср", "чт", "пт", "сб"][d.getDay()]} ${hm}`;
  return `${d.getDate()}.${String(d.getMonth() + 1).padStart(2, "0")} ${hm}`;
}
