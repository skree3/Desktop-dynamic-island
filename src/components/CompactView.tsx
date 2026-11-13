// Компактное состояние: самая приоритетная активность (таймер, музыка, плагин) или часы.

import { memo, useEffect } from "react";
import { ArrowDownToLine, Battery, BatteryCharging, BellOff, Cpu, ExternalLink, Hourglass, Mic, MicOff, Music2, Phone, Puzzle, Rocket, Timer as TimerIcon } from "lucide-react";
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

/** Запуск приложения из острова: иконка «вылетает», от неё расходится кольцо, по острову проходит блик. */
function LaunchPulse({ activity }: { activity: Activity }) {
  return (
    <div className="compact-row launch-pulse" key={activity.updatedAt}>
      <span className="launch-icon">
        {activity.icon ? <img src={activity.icon} alt="" draggable={false} /> : <Rocket size={16} />}
        <i className="launch-ring" />
        {activity.icon && <img className="launch-ghost" src={activity.icon} alt="" draggable={false} />}
      </span>
      <div className="compact-text">
        <span>{activity.title}</span>
      </div>
      <span className="launch-dots" aria-label={activity.subtitle ?? undefined}>
        <i />
        <i />
        <i />
      </span>
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
  let content;
  if (greeting) content = <Greeting text={greeting} />;
  else if (top?.id === "launch") content = <LaunchPulse activity={top} />;
  else if (top?.id === "call") content = <CallCompact activity={top} />;
  else if (top?.id === "download") content = <DownloadCompact activity={top} />;
  else if (top?.id === "battery") content = <BatteryPulse activity={top} />;
  else if (top?.id.startsWith("timer:")) content = <TimerCompact activity={top} />;
  else if (top?.kind === "music" && musicInCompact) content = <MusicCompact />;
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
