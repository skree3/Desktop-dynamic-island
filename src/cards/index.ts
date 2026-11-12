// Реестр карточек. Каждая карточка — отдельный ленивый чанк: код и данные
// карточки загружаются только когда её открывают, и освобождаются при закрытии.

import { lazy, type ComponentType, type LazyExoticComponent } from "react";
import {
  AppWindow,
  Bell,
  Bluetooth,
  Bot,
  Clipboard,
  Cpu,
  Music2,
  Plug,
  Puzzle,
  StickyNote,
  Inbox,
  SlidersHorizontal,
  Timer as TimerIcon,
  type LucideIcon,
} from "lucide-react";

export interface CardProps {
  cardId: string;
  pluginId?: string | null;
}

export interface CardMeta {
  icon: LucideIcon;
  component: LazyExoticComponent<ComponentType<CardProps>>;
}

export const CARD_META: Record<string, CardMeta> = {
  music: { icon: Music2, component: lazy(() => import("./MusicCard")) },
  notifications: { icon: Bell, component: lazy(() => import("./NotificationsCard")) },
  timers: { icon: TimerIcon, component: lazy(() => import("./TimersCard")) },
  quick: { icon: SlidersHorizontal, component: lazy(() => import("./QuickCard")) },
  system: { icon: Cpu, component: lazy(() => import("./SystemCard")) },
  bluetooth: { icon: Bluetooth, component: lazy(() => import("./BluetoothCard")) },
  clipboard: { icon: Clipboard, component: lazy(() => import("./ClipboardCard")) },
  apps: { icon: AppWindow, component: lazy(() => import("./AppsCard")) },
  ai: { icon: Bot, component: lazy(() => import("./AiCard")) },
  integrations: { icon: Plug, component: lazy(() => import("./IntegrationsCard")) },
  notes: { icon: StickyNote, component: lazy(() => import("./NotesCard")) },
  shelf: { icon: Inbox, component: lazy(() => import("./ShelfCard")) },
};

export const PLUGIN_META: CardMeta = { icon: Puzzle, component: lazy(() => import("./PluginCard")) };

export function metaFor(cardId: string): CardMeta | null {
  if (cardId.startsWith("plugin:")) return PLUGIN_META;
  return CARD_META[cardId] ?? null;
}
