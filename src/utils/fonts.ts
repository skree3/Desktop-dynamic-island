// Шрифты интерфейса: системный, 9 встроенных (с кириллицей, вшиты в приложение —
// интернет не нужен) и свои TTF/OTF из папки программы.

import "@fontsource-variable/inter/wght.css";
import "@fontsource-variable/manrope/wght.css";
import "@fontsource-variable/onest/wght.css";
import "@fontsource-variable/golos-text/wght.css";
import "@fontsource-variable/montserrat/wght.css";
import "@fontsource-variable/rubik/wght.css";
import "@fontsource-variable/nunito/wght.css";
import "@fontsource-variable/ibm-plex-sans/wght.css";
import "@fontsource-variable/comfortaa/wght.css";
import { ipc } from "../services/ipc";

export const SYSTEM_STACK = `"Segoe UI Variable Text", "Segoe UI", "Inter", "Cantarell", "Ubuntu", system-ui, sans-serif`;

export interface FontDef {
  id: string;
  label: string;
  family: string;
}

export const FONTS: FontDef[] = [
  { id: "system", label: "Системный", family: SYSTEM_STACK },
  { id: "inter", label: "Inter", family: `"Inter Variable"` },
  { id: "manrope", label: "Manrope", family: `"Manrope Variable"` },
  { id: "onest", label: "Onest", family: `"Onest Variable"` },
  { id: "golos", label: "Golos", family: `"Golos Text Variable"` },
  { id: "montserrat", label: "Montserrat", family: `"Montserrat Variable"` },
  { id: "rubik", label: "Rubik", family: `"Rubik Variable"` },
  { id: "nunito", label: "Nunito", family: `"Nunito Variable"` },
  { id: "plex", label: "IBM Plex Sans", family: `"IBM Plex Sans Variable"` },
  { id: "comfortaa", label: "Comfortaa", family: `"Comfortaa Variable"` },
];

const loaded = new Map<string, Promise<string>>();

/** Подключает свой шрифт через FontFace. Отклоняется, если браузер не смог его разобрать. */
export function loadCustomFont(file: string): Promise<string> {
  let p = loaded.get(file);
  if (!p) {
    p = (async () => {
      const url = await ipc.fontData(file);
      const family = `DI Custom ${file.replace(/\.[^.]+$/, "").replace(/[^\p{L}\p{N} _-]/gu, "")}`;
      const face = new FontFace(family, `url(${url})`);
      await face.load();
      document.fonts.add(face);
      return `"${family}"`;
    })();
    loaded.set(file, p);
    p.catch(() => loaded.delete(file));
  }
  return p;
}

/** CSS-стек для настройки `font`. Неизвестное или сломанное — системный. */
export async function resolveFont(id: string): Promise<string> {
  if (id.startsWith("custom:")) {
    try {
      return `${await loadCustomFont(id.slice(7))}, ${SYSTEM_STACK}`;
    } catch {
      return SYSTEM_STACK;
    }
  }
  const f = FONTS.find((x) => x.id === id);
  return f && f.id !== "system" ? `${f.family}, ${SYSTEM_STACK}` : SYSTEM_STACK;
}

/** Применяет шрифт ко всему окну. font-size-adjust выравнивает видимый размер
 *  букв под системный шрифт, чтобы смена шрифта не ломала вёрстку. */
export async function applyFont(id: string) {
  const stack = await resolveFont(id);
  const root = document.documentElement;
  root.style.setProperty("--font", stack);
  root.style.setProperty("--font-adjust", id === "system" ? "none" : "0.5");
}

/** Проверка, что свой шрифт действительно рисует текст (а не подменяется запасным). */
export async function verifyCustomFont(file: string): Promise<boolean> {
  try {
    const fam = await loadCustomFont(file);
    const c = document.createElement("canvas").getContext("2d");
    if (!c) return true;
    const sample = "Остров Island 0123";
    c.font = `32px monospace`;
    const base = c.measureText(sample).width;
    c.font = `32px ${fam}, monospace`;
    return Math.abs(c.measureText(sample).width - base) > 0.5;
  } catch {
    return false;
  }
}
