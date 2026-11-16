// Свой стиль из 1–3 цветов: акцент, оттенок фона и оттенок текста.
// Палитры для тёмного и светлого режимов строятся так, чтобы фон оставался
// почти нейтральным (цвет только «подкрашивает» его), а акцент был читаемым.

type RGB = [number, number, number];

function parse(hex: string): RGB {
  let h = hex.replace("#", "").trim();
  if (h.length === 3) h = h.split("").map((c) => c + c).join("");
  const n = parseInt(h.slice(0, 6), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

function hex([r, g, b]: RGB): string {
  return "#" + [r, g, b].map((v) => Math.round(Math.max(0, Math.min(255, v))).toString(16).padStart(2, "0")).join("");
}

/** Смешивает цвет `a` с `b`: t = 0 → a, t = 1 → b. */
export function mix(a: string, b: string, t: number): string {
  const x = parse(a);
  const y = parse(b);
  return hex([x[0] + (y[0] - x[0]) * t, x[1] + (y[1] - x[1]) * t, x[2] + (y[2] - x[2]) * t]);
}

export function luminance(c: string): number {
  const ch = parse(c).map((v) => {
    const s = v / 255;
    return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
  });
  return 0.2126 * ch[0] + 0.7152 * ch[1] + 0.0722 * ch[2];
}

export function contrast(a: string, b: string): number {
  const [l1, l2] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (l1 + 0.05) / (l2 + 0.05);
}

/** Текст поверх акцента: чёрный или белый — что контрастнее. */
function onAccent(accent: string): string {
  return contrast(accent, "#141415") >= contrast(accent, "#ffffff") ? "#141415" : "#ffffff";
}

/** Акцент, читаемый как текст на светлом фоне (контраст ≥ 4.5). */
function readableOnLight(accent: string, bg: string): string {
  let c = accent;
  for (let i = 0; i < 12 && contrast(c, bg) < 4.5; i++) c = mix(c, "#000000", 0.12);
  return c;
}

export interface Palette {
  bg: string;
  surface: string;
  surfaceHover: string;
  border: string;
  text: string;
  textMuted: string;
  accent: string;
  accentText: string;
  danger: string;
  success: string;
  warning: string;
}

export function buildPalettes(accent: string, base?: string | null, tint?: string | null): { dark: Palette; light: Palette } {
  const b = base || accent;
  const t = tint || null;
  const dark: Palette = {
    bg: mix("#111113", b, base ? 0.1 : 0.04),
    surface: mix("#1a1a1d", b, base ? 0.12 : 0.05),
    surfaceHover: mix("#252528", b, base ? 0.14 : 0.06),
    border: "rgba(237, 237, 237, 0.08)",
    text: t ? mix("#f0f0f2", t, 0.12) : "#ededed",
    textMuted: t ? mix("#a1a1a6", t, 0.3) : "#a1a1a6",
    accent,
    accentText: onAccent(accent),
    danger: "#e5484d",
    success: "#4cc38a",
    warning: "#f0a64a",
  };
  const lightBg = mix("#f6f6f7", b, base ? 0.07 : 0.03);
  const lightAccent = readableOnLight(accent, lightBg);
  const light: Palette = {
    bg: lightBg,
    surface: mix("#ffffff", b, base ? 0.04 : 0.015),
    surfaceHover: mix("#ececee", b, base ? 0.08 : 0.03),
    border: "rgba(23, 23, 23, 0.08)",
    text: t ? mix("#171717", t, 0.12) : "#171717",
    textMuted: t ? mix("#5e5e66", t, 0.3) : "#5e5e66",
    accent: lightAccent,
    accentText: onAccent(lightAccent),
    danger: "#dc2626",
    success: "#16a34a",
    warning: "#b86e14",
  };
  return { dark, light };
}

/** #rrggbb из любого цвета палитры (#rgb, #rrggbbaa, rgb[a]()) — для полей выбора цвета. */
export function toHex6(c: string | undefined, fallback = "#808080"): string {
  if (!c) return fallback;
  const v = c.trim();
  if (v.startsWith("#")) {
    let h = v.slice(1);
    if (h.length === 3 || h.length === 4) h = h.split("").map((x) => x + x).join("");
    return /^[0-9a-fA-F]{6}/.test(h) ? `#${h.slice(0, 6).toLowerCase()}` : fallback;
  }
  const m = v.match(/^rgba?\(([^)]+)\)$/);
  if (!m) return fallback;
  const [r, g, b] = m[1].split(",").map((x) => parseFloat(x));
  return hex([r, g, b]);
}

/** Прозрачность цвета палитры (1 — непрозрачный). */
export function alphaOf(c: string | undefined): number {
  if (!c) return 1;
  const v = c.trim();
  if (v.startsWith("#") && (v.length === 9 || v.length === 5)) {
    const a = v.length === 9 ? v.slice(7, 9) : v.slice(4, 5).repeat(2);
    return parseInt(a, 16) / 255;
  }
  const m = v.match(/^rgba\(([^)]+)\)$/);
  return m ? parseFloat(m[1].split(",")[3] ?? "1") : 1;
}

/** Цвет с прозрачностью: #rrggbb при a = 1, иначе rgba(). */
export function withAlpha(hex6: string, a: number): string {
  if (a >= 0.999) return hex6;
  const [r, g, b] = parse(hex6);
  return `rgba(${r}, ${g}, ${b}, ${Math.round(a * 1000) / 1000})`;
}

function hsl([r, g, b]: RGB): { h: number; s: number; l: number } {
  const x = r / 255, y = g / 255, z = b / 255;
  const max = Math.max(x, y, z), min = Math.min(x, y, z);
  const l = (max + min) / 2;
  if (max === min) return { h: 0, s: 0, l };
  const d = max - min;
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  const h = max === x ? (y - z) / d + (y < z ? 6 : 0) : max === y ? (z - x) / d + 2 : (x - y) / d + 4;
  return { h: h * 60, s, l };
}

/** Основные цвета картинки: самый частый (оттенок фона), самый яркий насыщенный (акцент)
 *  и второй насыщенный другого тона (оттенок текста). Всё считается локально. */
export async function paletteFromImage(file: File): Promise<{ accent: string; base: string; tint: string | null }> {
  const url = URL.createObjectURL(file);
  try {
    const img = await new Promise<HTMLImageElement>((res, rej) => {
      const i = new Image();
      i.onload = () => res(i);
      i.onerror = () => rej(new Error("Не удалось открыть картинку"));
      i.src = url;
    });
    const W = 72;
    const H = Math.max(1, Math.round((W * img.naturalHeight) / Math.max(1, img.naturalWidth)));
    const canvas = document.createElement("canvas");
    canvas.width = W;
    canvas.height = H;
    const ctx = canvas.getContext("2d", { willReadFrequently: true })!;
    ctx.drawImage(img, 0, 0, W, H);
    const px = ctx.getImageData(0, 0, W, H).data;
    const buckets = new Map<number, { r: number; g: number; b: number; n: number }>();
    for (let i = 0; i < px.length; i += 4) {
      if (px[i + 3] < 128) continue;
      const key = ((px[i] >> 4) << 8) | ((px[i + 1] >> 4) << 4) | (px[i + 2] >> 4);
      const e = buckets.get(key) ?? { r: 0, g: 0, b: 0, n: 0 };
      e.r += px[i];
      e.g += px[i + 1];
      e.b += px[i + 2];
      e.n += 1;
      buckets.set(key, e);
    }
    const cols = [...buckets.values()].map((e) => {
      const rgb: RGB = [e.r / e.n, e.g / e.n, e.b / e.n];
      return { hex: hex(rgb), n: e.n, ...hsl(rgb) };
    });
    if (!cols.length) throw new Error("В картинке нет цветов");
    const base = cols.reduce((a, b) => (b.n > a.n ? b : a));
    const score = (c: (typeof cols)[number]) => Math.sqrt(c.n) * c.s * (1 - Math.abs(c.l - 0.55) * 1.6);
    const ranked = cols.filter((c) => c.s > 0.2 && c.l > 0.2 && c.l < 0.85).sort((a, b) => score(b) - score(a));
    const accent = ranked[0] ?? base;
    const hueGap = (a: number, b: number) => Math.min(Math.abs(a - b), 360 - Math.abs(a - b));
    const tint = ranked.find((c) => hueGap(c.h, accent.h) > 40) ?? null;
    return { accent: accent.hex, base: base.hex, tint: tint?.hex ?? null };
  } finally {
    URL.revokeObjectURL(url);
  }
}
