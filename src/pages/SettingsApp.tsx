// Окно настроек (отдельный WebView, создаётся по требованию и уничтожается при закрытии).
// Разделы собраны в пять групп-«островов» слева: открытый остров раскрывается
// и показывает свои разделы, содержимое раздела справа мягко выезжает.

import { lazy, Suspense, useEffect, useState } from "react";
import { Bell, ChevronRight, Cpu, Plug, Shield, type LucideIcon } from "lucide-react";
import { Spinner } from "../components/ui";
import { useEvent } from "../hooks/common";
import { useTheme } from "../hooks/useTheme";
import { events, ipc } from "../services/ipc";
import { useApp } from "../stores/app";
import "../styles/settings.css";

type SectionDef = { id: string; title: string; component: React.LazyExoticComponent<React.ComponentType> };

const S = (loader: () => Promise<Record<string, React.ComponentType>>, name: string) =>
  lazy(() => loader().then((m) => ({ default: m[name] })));
const general = () => import("../settings/sections/general");
const modules = () => import("../settings/sections/modules");
const integrations = () => import("../settings/sections/integrations");
const plugins = () => import("../settings/sections/plugins");
const personal = () => import("../settings/sections/personal");
const system = () => import("../settings/sections/system");
const phone = () => import("../settings/sections/phone");
const calendar = () => import("../settings/sections/calendar");
const speech = () => import("../settings/sections/speech");
const extras = () => import("../settings/sections/extras");

/** Значок группы «Остров» — сама пилюля. */
function IslandIcon({ size = 18 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.5} aria-hidden>
      <rect x="3" y="8" width="18" height="8" rx="4" />
    </svg>
  );
}

type GroupDef = { id: string; title: string; icon: LucideIcon | typeof IslandIcon; sections: SectionDef[] };

export const GROUPS: GroupDef[] = [
  {
    id: "island",
    title: "Остров",
    icon: IslandIcon,
    sections: [
      { id: "general", title: "Общие", component: S(general, "GeneralSection") },
      { id: "appearance", title: "Внешний вид", component: S(general, "AppearanceSection") },
      { id: "position", title: "Положение", component: S(general, "PositionSection") },
      { id: "cards", title: "Карточки", component: S(modules, "CardsSection") },
      { id: "themes", title: "Темы", component: S(personal, "ThemesSection") },
      { id: "fonts", title: "Шрифты", component: S(extras, "FontsSection") },
      { id: "indicators", title: "Индикаторы", component: S(extras, "IndicatorsSection") },
      { id: "pomodoro", title: "Помодоро", component: S(calendar, "PomodoroSection") },
    ],
  },
  {
    id: "media",
    title: "Уведомления и медиа",
    icon: Bell,
    sections: [
      { id: "notifications", title: "Уведомления", component: S(modules, "NotificationsSection") },
      { id: "sounds", title: "Звуки", component: S(extras, "SoundsSection") },
      { id: "music", title: "Музыка", component: S(modules, "MusicSection") },
      { id: "clipboard", title: "Буфер обмена", component: S(modules, "ClipboardSection") },
    ],
  },
  {
    id: "links",
    title: "Подключения",
    icon: Plug,
    sections: [
      { id: "phone", title: "Телефон", component: S(phone, "PhoneSection") },
      { id: "vpn", title: "VPN", component: S(extras, "VpnSection") },
      { id: "calendar", title: "Календарь", component: S(calendar, "CalendarSection") },
      { id: "integrations", title: "Интеграции", component: S(integrations, "IntegrationsSection") },
      { id: "plugins", title: "Плагины", component: S(plugins, "PluginsSection") },
      { id: "hotkeys", title: "Горячие клавиши", component: S(personal, "HotkeysSection") },
    ],
  },
  {
    id: "system",
    title: "Система",
    icon: Cpu,
    sections: [
      { id: "system-monitor", title: "Системный монитор", component: S(modules, "SystemMonitorSection") },
      { id: "game-mode", title: "Игровой режим", component: S(system, "GameModeSection") },
      { id: "performance", title: "Производительность", component: S(system, "PerformanceSection") },
      { id: "speech", title: "Распознавание речи", component: S(speech, "SpeechSection") },
      { id: "monitors", title: "Мониторы", component: S(system, "MonitorsSection") },
      { id: "profiles", title: "Профили", component: S(personal, "ProfilesSection") },
    ],
  },
  {
    id: "privacy",
    title: "Приватность",
    icon: Shield,
    sections: [
      { id: "security", title: "Безопасность", component: S(system, "SecuritySection") },
      { id: "logs", title: "Журнал событий", component: S(system, "LogsSection") },
      { id: "about", title: "О программе", component: S(system, "AboutSection") },
      { id: "author", title: "Об авторе", component: S(extras, "AuthorSection") },
    ],
  },
];

const ALL = GROUPS.flatMap((g) => g.sections.map((s) => ({ ...s, group: g.id })));
export const SECTIONS = ALL;

function groupOf(section: string): string {
  return ALL.find((s) => s.id === section)?.group ?? "island";
}

export default function SettingsApp({ initialSection }: { initialSection: string | null }) {
  const ready = useApp((s) => s.ready);
  const [section, setSection] = useState(ALL.some((s) => s.id === initialSection) ? initialSection! : "notifications");
  const [open, setOpen] = useState<string | null>(groupOf(section));
  useTheme("settings");
  useEffect(() => {
    useApp.getState().init().catch(console.error);
  }, []);
  const wallpaper = useApp((s) => s.settings?.appearance.wallpaper ?? null);
  const [wallUrl, setWallUrl] = useState<string | null>(null);
  useEffect(() => {
    if (!wallpaper) return setWallUrl(null);
    ipc.wallpaperData().then(setWallUrl).catch(() => setWallUrl(null));
  }, [wallpaper]);
  // Тема могла быть создана в этом же окне — подтягиваем список.
  useEvent(events.THEME_CHANGED, () => ipc.themes().then((themes) => useApp.setState({ themes })).catch(() => {}));
  useEvent<string>(events.SETTINGS_NAVIGATE, (s) => {
    if (ALL.some((x) => x.id === s)) {
      setSection(s);
      setOpen(groupOf(s));
    }
  });

  if (!ready) return <div className="settings-loading"><Spinner size={22} /></div>;
  const current = ALL.find((s) => s.id === section)!;
  const Comp = current.component;
  return (
    <div className={`settings ${wallUrl ? "has-wall" : ""}`}>
      {wallUrl && <div className="settings-wall" style={{ backgroundImage: `url(${wallUrl})` }} aria-hidden />}
      <nav className="isles scroll" aria-label="Разделы настроек">
        {GROUPS.map((g) => {
          const isOpen = open === g.id;
          const Icon = g.icon;
          return (
            <div key={g.id} className={`isle ${isOpen ? "open" : ""}`}>
              <button type="button" className="isle-head" aria-expanded={isOpen} onClick={() => setOpen(isOpen ? null : g.id)}>
                <Icon size={18} />
                <span className="isle-title">{g.title}</span>
                <span className="isle-count">{g.sections.length}</span>
                <ChevronRight size={14} className="isle-chev" />
              </button>
              {isOpen && (
                <div className="isle-list">
                  {g.sections.map((s) => (
                    <button
                      key={s.id}
                      type="button"
                      className={`isle-item ${s.id === section ? "on" : ""}`}
                      aria-current={s.id === section ? "page" : undefined}
                      onClick={() => setSection(s.id)}
                    >
                      {s.title}
                    </button>
                  ))}
                </div>
              )}
            </div>
          );
        })}
      </nav>
      <main className="content scroll">
        <Suspense fallback={<div className="center"><Spinner /></div>}>
          <div className="section-enter" key={current.id}>
            <Comp />
          </div>
        </Suspense>
      </main>
    </div>
  );
}
