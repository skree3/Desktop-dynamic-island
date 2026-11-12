// Окно настроек (отдельный WebView, создаётся по требованию и уничтожается при закрытии).

import { lazy, Suspense, useEffect, useState } from "react";
import {
  Bell,
  Clipboard,
  Cpu,
  Gamepad2,
  Gauge,
  Info,
  Keyboard,
  LayoutGrid,
  Monitor,
  Move,
  Music2,
  Paintbrush,
  Palette,
  Plug,
  Puzzle,
  ScrollText,
  Settings2,
  Shield,
  Users,
  type LucideIcon,
} from "lucide-react";
import { Spinner } from "../components/ui";
import { useEvent } from "../hooks/common";
import { useTheme } from "../hooks/useTheme";
import { events } from "../services/ipc";
import { useApp } from "../stores/app";
import "../styles/settings.css";

type SectionDef = { id: string; title: string; icon: LucideIcon; component: React.LazyExoticComponent<React.ComponentType> };

const S = (loader: () => Promise<Record<string, React.ComponentType>>, name: string) =>
  lazy(() => loader().then((m) => ({ default: m[name] })));
const general = () => import("../settings/sections/general");
const modules = () => import("../settings/sections/modules");
const integrations = () => import("../settings/sections/integrations");
const plugins = () => import("../settings/sections/plugins");
const personal = () => import("../settings/sections/personal");
const system = () => import("../settings/sections/system");

export const SECTIONS: SectionDef[] = [
  { id: "general", title: "Общие", icon: Settings2, component: S(general, "GeneralSection") },
  { id: "appearance", title: "Внешний вид", icon: Paintbrush, component: S(general, "AppearanceSection") },
  { id: "position", title: "Положение панели", icon: Move, component: S(general, "PositionSection") },
  { id: "cards", title: "Карточки", icon: LayoutGrid, component: S(modules, "CardsSection") },
  { id: "notifications", title: "Уведомления", icon: Bell, component: S(modules, "NotificationsSection") },
  { id: "music", title: "Музыка", icon: Music2, component: S(modules, "MusicSection") },
  { id: "system-monitor", title: "Системный монитор", icon: Cpu, component: S(modules, "SystemMonitorSection") },
  { id: "clipboard", title: "Буфер обмена", icon: Clipboard, component: S(modules, "ClipboardSection") },
  { id: "integrations", title: "Интеграции", icon: Plug, component: S(integrations, "IntegrationsSection") },
  { id: "plugins", title: "Плагины", icon: Puzzle, component: S(plugins, "PluginsSection") },
  { id: "profiles", title: "Профили", icon: Users, component: S(personal, "ProfilesSection") },
  { id: "themes", title: "Темы", icon: Palette, component: S(personal, "ThemesSection") },
  { id: "hotkeys", title: "Горячие клавиши", icon: Keyboard, component: S(personal, "HotkeysSection") },
  { id: "game-mode", title: "Игровой режим", icon: Gamepad2, component: S(system, "GameModeSection") },
  { id: "performance", title: "Производительность", icon: Gauge, component: S(system, "PerformanceSection") },
  { id: "monitors", title: "Несколько мониторов", icon: Monitor, component: S(system, "MonitorsSection") },
  { id: "security", title: "Безопасность", icon: Shield, component: S(system, "SecuritySection") },
  { id: "logs", title: "Журнал событий", icon: ScrollText, component: S(system, "LogsSection") },
  { id: "about", title: "О программе", icon: Info, component: S(system, "AboutSection") },
];

export default function SettingsApp({ initialSection }: { initialSection: string | null }) {
  const ready = useApp((s) => s.ready);
  const [section, setSection] = useState(SECTIONS.some((s) => s.id === initialSection) ? initialSection! : "general");
  useTheme("settings");
  useEffect(() => {
    useApp.getState().init().catch(console.error);
  }, []);
  useEvent<string>(events.SETTINGS_NAVIGATE, (s) => SECTIONS.some((x) => x.id === s) && setSection(s));

  if (!ready) return <div className="settings-loading"><Spinner size={22} /></div>;
  const current = SECTIONS.find((s) => s.id === section)!;
  const Comp = current.component;
  return (
    <div className="settings">
      <nav className="sidebar">
        <div className="brand">
          <span className="brand-pill" />
          <span>Desktop Island</span>
        </div>
        {SECTIONS.map((s) => (
          <button key={s.id} type="button" className={`nav-item ${s.id === section ? "on" : ""}`} onClick={() => setSection(s.id)}>
            <s.icon size={16} />
            <span>{s.title}</span>
          </button>
        ))}
      </nav>
      <main className="content scroll">
        <Suspense fallback={<div className="center"><Spinner /></div>}>
          <Comp key={current.id} />
        </Suspense>
      </main>
    </div>
  );
}

