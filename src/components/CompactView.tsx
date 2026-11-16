// Компактное состояние: самая приоритетная активность (таймер, музыка, плагин) или часы.

import { memo, useEffect } from "react";
import { ArrowDownToLine, Battery, CalendarClock, Target, BatteryCharging, BellOff, Cpu, ExternalLink, Hourglass, Mic, MicOff, Music2, Phone, Plane, Puzzle, Rocket, ShieldCheck, ShieldOff, Timer as TimerIcon, Sun, Volume1, Volume2, VolumeX } from "lucide-react";
import { useNow } from "../hooks/common";
import { currentLine, useLyrics } from "../hooks/useLyrics";
import { ipc } from "../services/ipc";
import { useApp } from "../stores/app";
import { useIsland } from "../stores/island";
import { currentSession, useMedia } from "../stores/media";
import { useSys } from "../stores/sysmon";
import { timerElapsed, timerRemaining, useTimers } from "../stores/timers";
import type { Activity } from "../types";
import { formatClock, mediaPosition } from "../utils/format";

function Bars({ playing }: { playing: boolean }) {
  return (
    <span className={`eq ${playing ? "playing" : ""}`} aria-hidden>
      <i />
      <i />
      <i />
      <i />
    </span>
  );
}

/** Помодоро: фаза и обратный отсчёт до её конца. */
function PomodoroCompact({ activity }: { activity: Activity }) {
  const now = useNow(1000);
  const left = Math.max(0, (activity.startedAt ?? now) - now);
  const paused = activity.subtitle === "пауза";
  return (
    <div className="compact-row">
      <span className={`compact-icon ${activity.title.startsWith("Фокус") ? "accent" : ""}`}><Target size={15} /></span>
      <div className="compact-text">
        <span>{activity.title}</span>
        {paused && <small>пауза</small>}
      </div>
      <span className="compact-value mono">{paused ? "‖" : formatClock(left)}</span>
    </div>
  );
}

/** Приветствие: буквы проявляются по очереди. */
function Greeting({ text }: { text: string }) {
  return (
    <div className="compact-row greeting" aria-live="polite">
      <span className="greeting-dot" />
      <span className="greeting-text">
        {[...text].map((ch, i) => (
          <span key={i} style={{ animationDelay: `${i * 28}ms` }}>
            {ch === " " ? " " : ch}
          </span>
        ))}
      </span>
    </div>
  );
}

const MusicCompact = memo(function MusicCompact() {
  const sessions = useMedia((s) => s.sessions);
  const selected = useMedia((s) => s.selectedId);
  const artwork = useMedia((s) => s.artwork);
  const showArt = useApp((s) => s.settings?.music.showArtwork);
  const s = currentSession(sessions, selected);
  const lyrics = useLyrics(s);
  const playing = s?.status === "playing";
  // Пока есть синхронный текст и трек играет — обновляем строку 4 раза в секунду.
  const now = useNow(250, !!lyrics && playing);
  useEffect(() => {
    if (s && showArt) useMedia.getState().loadArtwork(s);
  }, [s?.artworkKey, showArt]);
  if (!s) return <Clock />;
  const art = artwork[s.artworkKey];
  const line = lyrics && playing ? currentLine(lyrics.lines, mediaPosition(s.positionMs, s.positionUpdatedAt, playing, s.durationMs, now) + 250) : null;
  return (
    <div className="compact-row">
      {showArt && art ? <img className="compact-art" src={art} alt="" draggable={false} /> : <span className="compact-icon"><Music2 size={15} /></span>}
      <div className="compact-text marquee">
        {line ? (
          <span
            className="lyric-line clickable"
            key={line}
            title="Текст песни на весь остров"
            data-no-drag
            onClick={(e) => {
              e.stopPropagation();
              useIsland.getState().expand("lyrics");
            }}
          >
            {line}
          </span>
        ) : (
          <span>{s.title || s.appName}</span>
        )}
      </div>
      <Bars playing={playing} />
    </div>
  );
});

/** Идёт звонок: приложение, длительность, микрофон и переход в приложение. */
function CallCompact({ activity }: { activity: Activity }) {
  const now = useNow(1000);
  const muted = useIsland((s) => s.privacy.micMuted);
  const elapsed = activity.startedAt ? Math.max(0, now - activity.startedAt) : 0;
  return (
    <div className="compact-row call-compact">
      <span className="call-icon">
        {activity.icon ? <img src={activity.icon} alt="" draggable={false} /> : <Phone size={14} />}
        <i className="call-pulse" />
      </span>
      <div className="compact-text">
        <span>{activity.title}</span>
      </div>
      <span className="compact-value mono call-time">{formatClock(elapsed)}</span>
      <button
        type="button"
        className={`call-btn ${muted ? "muted" : ""}`}
        title={muted ? "Включить микрофон" : "Выключить микрофон"}
        onClick={(e) => {
          e.stopPropagation();
          ipc.micToggle().catch(() => {});
        }}
      >
        {muted ? <MicOff size={13} /> : <Mic size={13} />}
      </button>
      <button
        type="button"
        className="call-btn"
        title={`Открыть ${activity.title}`}
        onClick={(e) => {
          e.stopPropagation();
          ipc.callOpen().catch(() => {});
        }}
      >
        <ExternalLink size={12} />
      </button>
    </div>
  );
}

/** Скачивание в браузере: имя файла, объём и скорость. */
function DownloadCompact({ activity }: { activity: Activity }) {
  return (
    <div className="compact-row download-compact">
      <span className="compact-icon accent download-icon">
        <ArrowDownToLine size={15} />
      </span>
      <div className="compact-text">
        <span>{activity.title}</span>
        {activity.subtitle && <small>{activity.subtitle}</small>}
      </div>
      <span className="download-bar" aria-hidden>
        <i />
      </span>
    </div>
  );
}

/** Точки как на iPhone: оранжевая — микрофон, зелёная — камера. */
function PrivacyDots() {
  const p = useIsland((s) => s.privacy);
  const on = useApp((s) => s.settings?.modules.privacy);
  if (!on || (!p.microphone.length && !p.camera.length)) return null;
  return (
    <span className="privacy-dots">
      {p.microphone.length > 0 && <i className="dot mic" title={`Микрофон: ${p.microphone.join(", ")}`} />}
      {p.camera.length > 0 && <i className="dot cam" title={`Камера: ${p.camera.join(", ")}`} />}
    </span>
  );
}

function TimerCompact({ activity }: { activity: Activity }) {
  const id = activity.id.slice("timer:".length);
  const timer = useTimers((s) => s.timers.find((t) => t.id === id));
  const now = useNow(activity.kind === "stopwatch" ? 100 : 500);
  if (!timer) return <Clock />;
  const countdown = timer.kind === "countdown";
  const value = countdown ? timerRemaining(timer, now) : timerElapsed(timer, now);
  const progress = countdown && timer.durationMs ? 1 - value / timer.durationMs : 0;
  return (
    <div className="compact-row">
      <span className="compact-icon accent">{countdown ? <Hourglass size={15} /> : <TimerIcon size={15} />}</span>
      <div className="compact-text">
        <span>{timer.label}</span>
      </div>
      <span className="compact-value mono">{formatClock(value)}</span>
      {countdown && (
        <svg className="ring" viewBox="0 0 20 20" width={18} height={18}>
          <circle cx="10" cy="10" r="8" className="ring-bg" />
          <circle cx="10" cy="10" r="8" className="ring-fg" strokeDasharray={`${progress * 50.27} 50.27`} />
        </svg>
      )}
    </div>
  );
}

/** Подключение/отключение зарядки: короткая анимация с процентом (как на iPhone). */
function BatteryPulse({ activity }: { activity: Activity }) {
  const charging = activity.kind === "charging";
  const pct = Math.round((activity.progress ?? 0) * 100);
  return (
    <div className={`compact-row battery-pulse ${charging ? "charging" : "unplugged"}`}>
      <span className="compact-icon">{charging ? <BatteryCharging size={17} /> : <Battery size={17} />}</span>
      <div className="compact-text">
        <span>{activity.title}</span>
      </div>
      <span className="battery-bar">
        <i style={{ width: `${pct}%` }} />
      </span>
      <span className="compact-value mono">{activity.subtitle}</span>
    </div>
  );
}

/** Запуск приложения из острова: иконка «вылетает», от неё расходится кольцо, по острову проходит блик.
 *  Запуск от имени администратора — со щитом и подписью. */
function LaunchPulse({ activity }: { activity: Activity }) {
  const admin = activity.kind === "launch-admin";
  return (
    <div className={`compact-row launch-pulse ${admin ? "admin" : ""}`} key={activity.updatedAt}>
      <span className="launch-icon">
        {activity.icon ? <img src={activity.icon} alt="" draggable={false} /> : <Rocket size={16} />}
        <i className="launch-ring" />
        {activity.icon && <img className="launch-ghost" src={activity.icon} alt="" draggable={false} />}
        {admin && <ShieldCheck className="launch-shield" size={11} />}
      </span>
      <div className="compact-text">
        <span>{activity.title}</span>
        {admin && <small>{activity.subtitle}</small>}
      </div>
      <span className="launch-dots" aria-label={activity.subtitle ?? undefined}>
        <i />
        <i />
        <i />
      </span>
    </div>
  );
}

/** Короткая вспышка: подключение и отключение VPN. */
function FlashCompact({ activity }: { activity: Activity }) {
  const connected = activity.title.includes("подключён");
  return (
    <div className={`compact-row flash flash-vpn ${connected ? "on" : ""}`} key={activity.updatedAt}>
      <span className="compact-icon">{connected ? <ShieldCheck size={15} /> : <ShieldOff size={15} />}</span>
      <div className="compact-text">
        <span>{activity.title}</span>
        {activity.subtitle && <small>{activity.subtitle}</small>}
      </div>
    </div>
  );
}

/** Режим полёта: самолётик летит по острову, пока режим включён. */
function AirplaneCompact() {
  return (
    <div className="compact-row airplane">
      <span className="sky" aria-hidden>
        <i className="cloud c1" />
        <i className="cloud c2" />
        <span className="plane"><Plane size={17} /></span>
      </span>
      <div className="compact-text"><span>Режим полёта</span></div>
    </div>
  );
}

/** Загрузка в Steam: игра, прогресс-кольцо, скорость и сколько осталось. */
function SteamCompact({ activity }: { activity: Activity }) {
  const p = Math.max(0, Math.min(1, activity.progress ?? 0));
  const [speed, ...rest] = (activity.subtitle ?? "").split(" · ");
  return (
    <div className="compact-row steam-compact">
      <span className="compact-icon steam-icon">
        <svg viewBox="0 0 24 24" width={15} height={15} aria-hidden>
          <circle cx="12" cy="12" r="10" fill="none" stroke="currentColor" strokeWidth="1.6" />
          <circle cx="15.5" cy="9" r="2.6" fill="none" stroke="currentColor" strokeWidth="1.6" />
          <circle cx="8.5" cy="15.5" r="1.8" fill="currentColor" />
          <path d="M9.8 14.8 13.6 10.6" stroke="currentColor" strokeWidth="1.6" />
        </svg>
      </span>
      <div className="compact-text">
        <span>{activity.title}</span>
        <small>{rest.join(" · ") || speed}</small>
      </div>
      <span className="compact-value mono steam-speed">{rest.length ? speed : ""}</span>
      <svg className="ring" viewBox="0 0 20 20" width={18} height={18} aria-label={`${Math.round(p * 100)}%`}>
        <circle cx="10" cy="10" r="8" className="ring-bg" />
        <circle cx="10" cy="10" r="8" className="ring-fg" strokeDasharray={`${p * 50.27} 50.27`} />
      </svg>
    </div>
  );
}

function Clock() {
  const now = useNow(1000);
  const showCpu = useApp((s) => s.settings?.systemMonitor.showInCompact && s.settings.modules.systemMonitor);
  const cpu = useSys((s) => s.snapshot?.cpu);
  const d = new Date(now);
  return (
    <div className="compact-row clock">
      <span className="clock-time mono">{d.toLocaleTimeString("ru-RU", { hour: "2-digit", minute: "2-digit" })}</span>
      <span className="clock-date">{d.toLocaleDateString("ru-RU", { weekday: "short", day: "numeric", month: "short" })}</span>
      {showCpu && (
        <span className="compact-metric">
          <Cpu size={12} /> {cpu != null ? `${Math.round(cpu)}%` : "—"}
        </span>
      )}
    </div>
  );
}

export function CompactView({ top }: { top: Activity | null }) {
  const dnd = useIsland((s) => s.notification.dnd);
  const queued = useIsland((s) => s.notification.queued);
  const musicInCompact = useApp((s) => s.settings?.music.showInCompact);
  const greeting = useIsland((s) => s.greeting);
  const osd = useIsland((s) => s.osd);
  let content;
  if (greeting) content = <Greeting text={greeting} />;
  if (osd) content = <OsdView o={osd} />;
  else if (top?.id === "launch") content = <LaunchPulse activity={top} />;
  else if (top?.id === "flash") content = <FlashCompact activity={top} />;
  else if (top?.id === "call") content = <CallCompact activity={top} />;
  else if (top?.id === "download") content = <DownloadCompact activity={top} />;
  else if (top?.id === "steam") content = <SteamCompact activity={top} />;
  else if (top?.id === "battery") content = <BatteryPulse activity={top} />;
  else if (top?.id.startsWith("timer:")) content = <TimerCompact activity={top} />;
  else if (top?.kind === "music" && musicInCompact) content = <MusicCompact />;
  else if (top?.id === "pomodoro") content = <PomodoroCompact activity={top} />;
  else if (top?.id === "airplane") content = <AirplaneCompact />;
  else if (top?.id === "calendar")
    content = (
      <div className="compact-row">
        <span className="compact-icon accent"><CalendarClock size={15} /></span>
        <div className="compact-text">
          <span>{top.title}</span>
          {top.subtitle && <small>{top.subtitle}</small>}
        </div>
      </div>
    );
  else if (top?.kind === "plugin")
    content = (
      <div className="compact-row">
        <span className="compact-icon"><Puzzle size={15} /></span>
        <div className="compact-text">
          <span>{top.title}</span>
          {top.subtitle && <small>{top.subtitle}</small>}
        </div>
        {top.progress != null && <span className="compact-value">{Math.round(top.progress * 100)}%</span>}
      </div>
    );
  else content = <Clock />;
  return (
    <div className="compact">
      {content}
      <PrivacyDots />
      {(dnd || queued > 0) && (
        <div className="compact-badges">
          {dnd && <BellOff size={12} aria-label="Не беспокоить" />}
          {queued > 0 && <span className="badge">+{queued}</span>}
        </div>
      )}
    </div>
  );
}

/** Громкость или яркость вместо системной плашки Windows. */
function OsdView({ o }: { o: import("../types").OsdPayload }) {
  const Icon = o.kind === "brightness" ? Sun : o.muted || o.value === 0 ? VolumeX : o.value < 50 ? Volume1 : Volume2;
  const value = o.kind === "volume" && o.muted ? 0 : o.value;
  return (
    <div className={`osd ${o.kind === "volume" && o.muted ? "muted" : ""}`} aria-live="polite">
      <Icon size={17} />
      <span className="osd-bar">
        <i style={{ width: `${value}%` }} />
      </span>
      <span className="osd-val">{o.kind === "volume" && o.muted ? "Без звука" : `${o.value}`}</span>
    </div>
  );
}
