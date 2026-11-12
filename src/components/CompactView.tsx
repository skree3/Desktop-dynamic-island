// Компактное состояние: самая приоритетная активность (таймер, музыка, плагин) или часы.

import { memo, useEffect } from "react";
import { Battery, BatteryCharging, BellOff, Cpu, Hourglass, Music2, Puzzle, Timer as TimerIcon } from "lucide-react";
import { useNow } from "../hooks/common";
import { useApp } from "../stores/app";
import { useIsland } from "../stores/island";
import { currentSession, useMedia } from "../stores/media";
import { useSys } from "../stores/sysmon";
import { timerElapsed, timerRemaining, useTimers } from "../stores/timers";
import type { Activity } from "../types";
import { formatClock } from "../utils/format";

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

const MusicCompact = memo(function MusicCompact() {
  const sessions = useMedia((s) => s.sessions);
  const selected = useMedia((s) => s.selectedId);
  const artwork = useMedia((s) => s.artwork);
  const showArt = useApp((s) => s.settings?.music.showArtwork);
  const s = currentSession(sessions, selected);
  useEffect(() => {
    if (s && showArt) useMedia.getState().loadArtwork(s);
  }, [s?.artworkKey, showArt]);
  if (!s) return <Clock />;
  const art = artwork[s.artworkKey];
  return (
    <div className="compact-row">
      {showArt && art ? <img className="compact-art" src={art} alt="" draggable={false} /> : <span className="compact-icon"><Music2 size={15} /></span>}
      <div className="compact-text marquee">
        <span>{s.title || s.appName}</span>
      </div>
      <Bars playing={s.status === "playing"} />
    </div>
  );
});

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
  let content;
  if (top?.id === "battery") content = <BatteryPulse activity={top} />;
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
      {(dnd || queued > 0) && (
        <div className="compact-badges">
          {dnd && <BellOff size={12} aria-label="Не беспокоить" />}
          {queued > 0 && <span className="badge">+{queued}</span>}
        </div>
      )}
    </div>
  );
}
