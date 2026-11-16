// Окно настроек (отдельный WebView, создаётся по требованию и уничтожается при закрытии).
// Разделы собраны в пять групп-«островов» слева: открытый остров раскрывается
// и показывает свои разделы, содержимое раздела справа мягко выезжает.

import { lazy, startTransition, Suspense, useEffect, useRef, useState } from "react";
import { Bell, ChevronRight, Cpu, Plug, Shield, type LucideIcon } from "lucide-react";
import { Spinner } from "../components/ui";
import { useEvent } from "../hooks/common";
import { useTheme } from "../hooks/useTheme";
import { events, ipc } from "../services/ipc";
import { useApp } from "../stores/app";
import "../styles/settings.css";

type Loader = () => Promise<Record<string, React.ComponentType>>;
type SectionDef = { id: string; title: string; component: React.LazyExoticComponent<React.ComponentType>; load: Loader };
/** Раздел: ленивый компонент и загрузчик его модуля (чтобы подгрузить заранее, без мигания). */
const S = (loader: Loader, name: string) => ({ component: lazy(() => loader().then((m) => ({ default: m[name] }))), load: loader });
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
      { id: "general", title: "Общие", ...S(general, "GeneralSection") },
      { id: "appearance", title: "Внешний вид", ...S(general, "AppearanceSection") },
      { id: "position", title: "Положение", ...S(general, "PositionSection") },
      { id: "cards", title: "Карточки", ...S(modules, "CardsSection") },
      { id: "themes", title: "Темы", ...S(personal, "ThemesSection") },
      { id: "fonts", title: "Шрифты", ...S(extras, "FontsSection") },
      { id: "indicators", title: "Индикаторы", ...S(extras, "IndicatorsSection") },
      { id: "pomodoro", title: "Помодоро", ...S(calendar, "PomodoroSection") },
    ],
  },
  {
    id: "media",
    title: "Уведомления и медиа",
    icon: Bell,
    sections: [
      { id: "notifications", title: "Уведомления", ...S(modules, "NotificationsSection") },
      { id: "sounds", title: "Звуки", ...S(extras, "SoundsSection") },
      { id: "music", title: "Музыка", ...S(modules, "MusicSection") },
      { id: "clipboard", title: "Буфер обмена", ...S(modules, "ClipboardSection") },
    ],
  },
  {
    id: "links",
    title: "Подключения",
    icon: Plug,
    sections: [
      { id: "phone", title: "Телефон", ...S(phone, "PhoneSection") },
      { id: "vpn", title: "VPN", ...S(extras, "VpnSection") },
      { id: "calendar", title: "Календарь", ...S(calendar, "CalendarSection") },
      { id: "integrations", title: "Интеграции", ...S(integrations, "IntegrationsSection") },
      { id: "plugins", title: "Плагины", ...S(plugins, "PluginsSection") },
      { id: "hotkeys", title: "Горячие клавиши", ...S(personal, "HotkeysSection") },
    ],
  },
  {
    id: "system",
    title: "Система",
    icon: Cpu,
    sections: [
      { id: "system-monitor", title: "Системный монитор", ...S(modules, "SystemMonitorSection") },
      { id: "game-mode", title: "Игровой режим", ...S(system, "GameModeSection") },
      { id: "performance", title: "Производительность", ...S(system, "PerformanceSection") },
      { id: "speech", title: "Распознавание речи", ...S(speech, "SpeechSection") },
      { id: "monitors", title: "Мониторы", ...S(system, "MonitorsSection") },
      { id: "profiles", title: "Профили", ...S(personal, "ProfilesSection") },
    ],
  },
  {
    id: "privacy",
    title: "Приватность",
    icon: Shield,
    sections: [
      { id: "security", title: "Безопасность", ...S(system, "SecuritySection") },
      { id: "logs", title: "Журнал событий", ...S(system, "LogsSection") },
      { id: "about", title: "О программе", ...S(system, "AboutSection") },
      { id: "author", title: "Об авторе", ...S(extras, "AuthorSection") },
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
  // Показываемый раздел отстаёт от выбранного на время анимации ухода.
  const [shown, setShown] = useState(section);
  const [leaving, setLeaving] = useState(false);
  const mainRef = useRef<HTMLElement>(null);
  useEffect(() => {
    if (section === shown) return;
    const next = ALL.find((s) => s.id === section);
    const calm = document.documentElement.dataset.animations === "off" || window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    setLeaving(!calm);
    let alive = true;
    // Модуль следующего раздела грузится, пока старый раздел уходит.
    const ready = Promise.all([next?.load().catch(() => null), new Promise((r) => setTimeout(r, calm ? 0 : 130))]);
    ready.then(() => {
      if (!alive) return;
      startTransition(() => {
        setShown(section);
        setLeaving(false);
      });
    });
    return () => {
      alive = false;
    };
  }, [section]);
  useEffect(() => {
    mainRef.current?.scrollTo({ top: 0 });
  }, [shown]);
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
  const current = ALL.find((s) => s.id === shown)!;
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
              <div className="isle-collapse" aria-hidden={!isOpen}>
                <div className="isle-clip">
                  <div className="isle-list">
                    {g.sections.map((s) => (
                      <button
                        key={s.id}
                        type="button"
                        tabIndex={isOpen ? 0 : -1}
                        className={`isle-item ${s.id === section ? "on" : ""}`}
                        aria-current={s.id === section ? "page" : undefined}
                        onClick={() => setSection(s.id)}
                      >
                        {s.title}
                      </button>
                    ))}
                  </div>
                </div>
              </div>
            </div>
          );
        })}
      </nav>
      <main className="content scroll" ref={mainRef}>
        <Suspense fallback={<div className="center"><Spinner /></div>}>
          <div className={leaving ? "section-leave" : "section-enter"} key={current.id}>
            <Comp />
          </div>
        </Suspense>
      </main>
    </div>
  );
}
