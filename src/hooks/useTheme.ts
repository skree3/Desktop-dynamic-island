// Применяет тему и параметры внешнего вида к CSS-переменным документа.

import { useEffect, useState } from "react";
import { useApp } from "../stores/app";
import { hexToRgba } from "../utils/format";

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
    root.style.setProperty("--island-bg", hexToRgba(palette.bg ?? "#000000", target === "island" ? appearance.opacity : 1));
    root.style.setProperty("--radius", `${appearance.radius}px`);
    root.style.setProperty("--scale", String(appearance.scale));
    root.dataset.mode = mode;
    root.dataset.animations = appearance.animations;
  }, [appearance, themes, mode, target]);
}
