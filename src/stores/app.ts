// Глобальное состояние приложения: настройки, карточки, темы, окружение.

import { create } from "zustand";
import { events, ipc, on } from "../services/ipc";
import type { CardInfo, DbStatus, EnvironmentInfo, Settings, SettingsSection, ThemeRow } from "../types";

interface AppStore {
  ready: boolean;
  version: string;
  os: string;
  settings: Settings | null;
  cards: CardInfo[];
  themes: ThemeRow[];
  environment: EnvironmentInfo | null;
  db: DbStatus | null;
  init: () => Promise<void>;
  update: <K extends SettingsSection>(section: K, patch: Partial<Settings[K]>) => Promise<void>;
  refreshCards: () => Promise<void>;
  refreshThemes: () => Promise<void>;
}

let subscribed = false;

export const useApp = create<AppStore>((set, get) => ({
  ready: false,
  version: "",
  os: "",
  settings: null,
  cards: [],
  themes: [],
  environment: null,
  db: null,

  init: async () => {
    const b = await ipc.bootstrap();
    set({
      ready: true,
      version: b.version,
      os: b.os,
      settings: b.settings,
      cards: b.cards,
      themes: b.themes,
      environment: b.environment,
      db: b.db,
    });
    ipc.setTimezone(-new Date().getTimezoneOffset()).catch(() => {});
    if (!subscribed) {
      subscribed = true;
      on<Settings>(events.SETTINGS_CHANGED, (s) => set({ settings: s }));
      on<CardInfo[]>(events.WIDGETS_CHANGED, () => get().refreshCards());
      on<ThemeRow[]>(events.THEME_CHANGED, (t) => set({ themes: t }));
      on(events.PLUGINS_CHANGED, () => get().refreshCards());
    }
    return void b;
  },

  update: async (section, patch) => {
    const prev = get().settings;
    if (prev) {
      // Оптимистичное обновление: UI откликается сразу, бэкенд валидирует и присылает итог.
      set({ settings: { ...prev, [section]: { ...prev[section], ...patch } } });
    }
    try {
      const next = await ipc.updateSettings(section, patch);
      set({ settings: next });
      if (section === "modules") get().refreshCards();
    } catch (e) {
      if (prev) set({ settings: prev });
      throw e;
    }
  },

  refreshCards: async () => {
    try {
      set({ cards: await ipc.cards() });
    } catch {
      /* окно могло закрыться */
    }
  },

  refreshThemes: async () => set({ themes: await ipc.themes() }),
}));

/** Карточка → ключ модуля в настройках (snake_case → camelCase). */
export function moduleKey(module: string): keyof Settings["modules"] {
  return module.replace(/_([a-z])/g, (_, c) => c.toUpperCase()) as keyof Settings["modules"];
}
