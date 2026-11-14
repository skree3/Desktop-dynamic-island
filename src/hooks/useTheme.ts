// Применяет тему и параметры внешнего вида к CSS-переменным документа.

import { useEffect, useState } from "react";
import { useApp } from "../stores/app";
import { hexToRgba } from "../utils/format";
import { setLanguage } from "../i18n";
import { applyFont } from "../utils/fonts";

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

export function useTheme(target: "island" | "settings") {
  const appearance = useApp((s) => s.settings?.appearance);
  const themes = useApp((s) => s.themes);
  const mode = useResolvedMode();
  const font = appearance?.font ?? "system";

  // Шрифт интерфейса — для всего окна сразу (остров, меню, уведомления, настройки).
  useEffect(() => {
    applyFont(font).catch(() => {});
  }, [font]);

  useEffect(() => {
    if (!appearance) return;
    const theme = themes.find((t) => t.id === appearance.themeId) ?? themes[0];
    const palette = theme?.data?.[mode] ?? {};
    const root = document.documentElement;
    const set = (k: string, v: string | undefined) => v && root.style.setProperty(k, v);
    set("--bg", palette.bg);
    set("--surface", palette.surface);
    set("--surface-hover", palette.surfaceHover);
    set("--border", palette.border);
    set("--text", palette.text);
    set("--text-muted", palette.textMuted);
    set("--accent", palette.accent ?? appearance.accent);
    set("--accent-text", palette.accentText);
    set("--danger", palette.danger);
    set("--success", palette.success);
    set("--warning", palette.warning);
    // Цвет самого острова: графит, серый или чёрный (в тёмном режиме).
    const tones: Record<string, string> = { graphite: "#1c1c1f", grey: "#2a2a2e", black: "#000000" };
    const islandBg = target === "island" && mode === "dark" ? tones[appearance.islandTone ?? "graphite"] ?? palette.bg : palette.bg;
    root.style.setProperty("--island-bg", hexToRgba(islandBg ?? "#000000", target === "island" ? appearance.opacity : 1));
    root.style.setProperty("--radius", `${appearance.radius}px`);
    if (target === "settings") {
      // Окно настроек следует выбранной теме (раньше было всегда графитово-янтарным).
      const op = appearance.settingsOpacity ?? 1;
      const bg = palette.bg ?? (mode === "dark" ? "#111112" : "#f5f5f6");
      set("--s-bg", hexToRgba(bg, op));
      set("--s-nav", "transparent");
      set("--s-isle", palette.surface);
      set("--s-isle-open", palette.surface);
      set("--s-surface", palette.surface);
      set("--s-raised", palette.surfaceHover);
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
  }, [appearance, themes, mode, target]);
}
