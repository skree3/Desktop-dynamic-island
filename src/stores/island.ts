// Состояние панели: режим отображения, активная панель, уведомления, активности.

import { create } from "zustand";
import type { Activity, GameModeState, NotificationPayload, PrivacyState, Priority } from "../types";

export type IslandMode = "compact" | "expanded" | "notification";

const PRIORITY_RANK: Record<Priority, number> = { low: 0, normal: 1, high: 2, critical: 3 };

/** Какая активность показывается в компактной панели (чистая функция — покрыта тестом). */
export function pickCompact(activities: Activity[]): Activity | null {
  if (!activities.length) return null;
  return [...activities].sort((a, b) => PRIORITY_RANK[b.priority] - PRIORITY_RANK[a.priority] || b.updatedAt - a.updatedAt)[0];
}

interface IslandStore {
  mode: IslandMode;
  activeCard: string | null;
  hovered: boolean;
  /** Панель прилегает к верхнему краю экрана (форма «чёлки»). */
  attached: boolean;
  /** Над островом перетаскивают файлы (полка). */
  dragging: boolean;
  notification: NotificationPayload;
  activities: Activity[];
  gameMode: GameModeState;
  /** Кто использует микрофон и камеру. */
  privacy: PrivacyState;
  /** Приветствие, показываемое в свёрнутом острове несколько секунд. */
  greeting: string | null;
  setGreeting: (g: string | null) => void;
  expand: (card?: string | null) => void;
  collapse: () => void;
  setCard: (card: string) => void;
  setHovered: (h: boolean) => void;
  setAttached: (a: boolean) => void;
  setDragging: (d: boolean) => void;
  setNotification: (n: NotificationPayload) => void;
  setActivities: (a: Activity[]) => void;
  setGameMode: (g: GameModeState) => void;
  setPrivacy: (p: PrivacyState) => void;
}

export const useIsland = create<IslandStore>((set, get) => ({
  mode: "compact",
  activeCard: null,
  hovered: false,
  attached: true,
  dragging: false,
  notification: { current: null, queued: 0, dnd: false, missed: 0 },
  activities: [],
  gameMode: { active: false, reason: null, app: null },
  privacy: { microphone: [], camera: [], micMuted: null },
  greeting: null,
  setGreeting: (greeting) => set({ greeting }),

  expand: (card) => set({ mode: "expanded", activeCard: card ?? get().activeCard }),
  collapse: () => {
    const n = get().notification.current;
    set({ mode: n ? "notification" : "compact" });
  },
  setCard: (card) => set({ activeCard: card }),
  setHovered: (hovered) => {
    if (get().hovered !== hovered) set({ hovered });
  },
  setAttached: (attached) => set({ attached }),
  setDragging: (dragging) => set({ dragging }),
  setPrivacy: (privacy) => set({ privacy }),
  setNotification: (notification) => {
    const mode = get().mode;
    // Уведомление не перебивает раскрытую панель — оно появится после сворачивания.
    if (notification.current && mode !== "expanded") set({ notification, mode: "notification" });
    else if (!notification.current && mode === "notification") set({ notification, mode: "compact" });
    else set({ notification });
  },
  setActivities: (activities) => set({ activities }),
  setGameMode: (gameMode) => set({ gameMode, ...(gameMode.active ? { mode: "compact" as const } : {}) }),
}));
