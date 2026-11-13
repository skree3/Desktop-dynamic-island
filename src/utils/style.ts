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
