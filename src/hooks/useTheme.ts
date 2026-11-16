// Применяет тему и параметры внешнего вида к CSS-переменным документа.

import { useEffect, useState } from "react";
import { useApp } from "../stores/app";
import { hexToRgba } from "../utils/format";
import { setLanguage } from "../i18n";
import { applyFont } from "../utils/fonts";
import type { ThemeRow } from "../types";

function systemDark(): boolean {
  return window.matchMedia?.("(prefers-color-scheme: dark)").matches ?? true;
}

export function useResolvedMode(): "dark" | "light" {
  const mode = useApp((s) => s.settings?.appearance.mode ?? "system");
  const [sys, setSys] = useState(systemDark());
  useEffect(() => {
    const mq = window.matchMedia?.("(prefers-color-scheme: dark)");
    if (!mq) return;
    const h = () => setSys(mq.matches);
    mq.addEventListener("change", h);
    return () => mq.removeEventListener("change", h);
  }, []);
  return mode === "system" ? (sys ? "dark" : "light") : mode;
}

/** Данные текущей темы (на острове — с учётом предпросмотра из редактора). */
export function useActiveTheme(target: "island" | "settings"): ThemeRow["data"] | null {
  const themeId = useApp((s) => s.settings?.appearance.themeId);
  const themes = useApp((s) => s.themes);
  const preview = useApp((s) => s.previewTheme);
  if (target === "island" && preview) return preview;
  const list = Array.isArray(themes) ? themes : [];
  return (list.find((t) => t.id === themeId) ?? list[0])?.data ?? null;
}

const gradient = (g: { colors: string[]; angle: number }, alpha: number) =>
  `linear-gradient(${g.angle}deg, ${g.colors.map((c) => hexToRgba(c, alpha)).join(", ")})`;

export function useTheme(target: "island" | "settings") {
  const appearance = useApp((s) => s.settings?.appearance);
  const data = useActiveTheme(target);
  const mode = useResolvedMode();
  const font = appearance?.font ?? "system";

  // Шрифт интерфейса — для всего окна сразу (остров, меню, уведомления, настройки).
  useEffect(() => {
    applyFont(font).catch(() => {});
  }, [font]);

  useEffect(() => {
    if (!appearance) return;
    const palette = data?.[mode] ?? {};
    const root = document.documentElement;
    const set = (k: string, v: string | undefined) => v && root.style.setProperty(k, v);
    const unset = (k: string) => root.style.removeProperty(k);
    // Прозрачность карточек своей темы.
    const cardAlpha = data?.cards?.opacity ?? 1;
    const card = (c: string | undefined) => (c && cardAlpha < 1 ? hexToRgba(c, cardAlpha) : c);
    set("--bg", palette.bg);
    set("--surface", card(palette.surface));
    set("--surface-hover", card(palette.surfaceHover));
    set("--border", palette.border);
    set("--text", palette.text);
    set("--text-muted", palette.textMuted);
    set("--accent", palette.accent ?? appearance.accent);
    set("--accent-text", palette.accentText);
    set("--danger", palette.danger);
    set("--success", palette.success);
    set("--warning", palette.warning);
    // Цвет самого острова: из своей темы, иначе графит, серый или чёрный (в тёмном режиме).
    const tones: Record<string, string> = { graphite: "#1c1c1f", grey: "#2a2a2e", black: "#000000" };
    const islandAlpha = target === "island" ? appearance.opacity : 1;
    const islandBg = data?.island?.color ?? (target === "island" && mode === "dark" ? tones[appearance.islandTone ?? "graphite"] ?? palette.bg : palette.bg);
    root.style.setProperty("--island-bg", hexToRgba(islandBg ?? "#000000", islandAlpha));
    if (data?.island?.border) root.style.setProperty("--island-ring", `inset 0 0 0 1px ${data.island.border}`);
    else unset("--island-ring");
    if (data?.island?.glow) root.style.setProperty("--island-glow", `0 0 18px ${hexToRgba(palette.accent ?? appearance.accent, 0.45)}`);
    else unset("--island-glow");
    if (data?.background) root.style.setProperty("--island-gradient", gradient(data.background, islandAlpha));
    else unset("--island-gradient");
    // Скругления своей темы; без них — значения по умолчанию из CSS.
    if (data?.shape) {
      root.style.setProperty("--r-card", `${data.shape.card}px`);
      root.style.setProperty("--r-btn", `${data.shape.button}px`);
    } else {
      unset("--r-card");
      unset("--r-btn");
    }
    if (data?.cards?.blur) root.style.setProperty("--card-blur", `blur(${data.cards.blur}px)`);
    else unset("--card-blur");
    root.style.setProperty("--radius", `${appearance.radius}px`);
    if (target === "settings") {
      // Окно настроек следует выбранной теме (раньше было всегда графитово-янтарным).
      const op = appearance.settingsOpacity ?? 1;
      const bg = palette.bg ?? (mode === "dark" ? "#111112" : "#f5f5f6");
      set("--s-bg", hexToRgba(bg, op));
      set("--s-nav", "transparent");
      set("--s-isle", card(palette.surface));
      set("--s-isle-open", card(palette.surface));
      set("--s-surface", card(palette.surface));
      set("--s-raised", card(palette.surfaceHover));
      if (data?.background) root.style.setProperty("--s-gradient", gradient(data.background, op));
      else unset("--s-gradient");
      set("--s-hair", palette.border);
      set("--s-text", palette.text);
      set("--s-text2", palette.textMuted);
      set("--s-muted", palette.textMuted);
      set("--s-accent", palette.accent ?? appearance.accent);
      set("--s-accent-text", palette.accentText);
      root.style.setProperty("--wall-dim", String(appearance.wallpaperDim ?? 0.45));
      root.style.setProperty("--wall-blur", `${appearance.wallpaperBlur ?? 12}px`);
    }
    root.style.setProperty("--scale", String(appearance.scale));
    root.dataset.mode = mode;
    setLanguage(appearance.language ?? "ru");
    root.dataset.animations = appearance.animations;
  }, [appearance, data, mode, target]);
}
