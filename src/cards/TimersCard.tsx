import { useEffect, useState } from "react";
import { Hourglass, Pause, Play, Plus, RotateCcw, SkipForward, Square, Timer as TimerIcon, Trash2 } from "lucide-react";
import { IconButton, Note, Segmented } from "../components/ui";
import { useAction, useEvent, useNow } from "../hooks/common";
import { ipc } from "../services/ipc";
import { timerElapsed, timerRemaining, useTimers } from "../stores/timers";
import type { PomodoroState, Timer } from "../types";
import { formatClock, formatStopwatch, parseDuration } from "../utils/format";

const PRESETS = [1, 5, 10, 25];

function Pomodoro() {
  const [s, setS] = useState<PomodoroState | null>(null);
  const now = useNow(1000);
  useEffect(() => {
    ipc.pomodoroStatus().then(setS).catch(() => {});
  }, []);
  useEvent<PomodoroState>("pomodoro://state", setS);
  if (!s) return null;
  const running = s.phase !== "idle";
  const left = s.pausedLeftMs ?? Math.max(0, s.endsAt - now);
  const max = Math.max(1, ...s.week);
  const phase = s.phase === "focus" ? `Фокус ${s.cycle} из ${s.cycles}` : s.phase === "short" ? "Короткий перерыв" : s.phase === "long" ? "Длинный перерыв" : "Помодоро";
  return (
    <div className="pomo">
      <div className="pomo-top">
        <div className="grow">
          <div className="pomo-time">{running ? formatClock(left) : "25:00"}</div>
          <div className="pomo-phase">{phase} · сегодня {s.today}</div>
        </div>
        {running && s.pausedLeftMs == null ? (
          <IconButton title="Пауза" onClick={() => ipc.pomodoro("pause").catch(() => {})}>
            <Pause size={16} />
          </IconButton>
        ) : (
          <IconButton title="Старт" onClick={() => ipc.pomodoro("start").catch(() => {})}>
            <Play size={16} />
          </IconButton>
        )}
        {running && (
          <IconButton title="Следующая фаза" onClick={() => ipc.pomodoro("skip").catch(() => {})}>
            <SkipForward size={16} />
          </IconButton>
        )}
        {running && (
          <IconButton title="Остановить" onClick={() => ipc.pomodoro("stop").catch(() => {})}>
            <Square size={14} />
          </IconButton>
        )}
      </div>
      {s.week.length > 0 && (
        <div className="pomo-week" title="Фокусов за неделю">
          {s.week.map((c, i) => (
            <i key={i} className={i === s.week.length - 1 ? "today" : ""} style={{ height: `${Math.max(12, (c / max) * 100)}%` }} />
          ))}
        </div>
      )}
    </div>
  );
}

export default function TimersCard() {
  const timers = useTimers((s) => s.timers);
  const anyRunning = timers.some((t) => t.state === "running");
  const now = useNow(anyRunning ? 100 : 1000);
  const [kind, setKind] = useState<"countdown" | "stopwatch">("countdown");
  const [duration, setDuration] = useState("5:00");
  const [label, setLabel] = useState("");
  const { error, run } = useAction();

  useEffect(() => {
    useTimers.getState().load();
  }, []);

  const create = (ms?: number) =>
    run(async () => {
      const d = kind === "countdown" ? (ms ?? parseDuration(duration)) : 0;
      if (kind === "countdown" && !d) throw new Error("Укажите длительность, например 5, 1:30 или 90s");
      await ipc.timerCreate(label, kind, d ?? 0, true);
      setLabel("");
    });

  const d = new Date(now);
  return (
    <div className="card-pad">
      <Pomodoro />
      <div className="clock-big">
        <span className="mono">{d.toLocaleTimeString("ru-RU")}</span>
        <small>{d.toLocaleDateString("ru-RU", { weekday: "long", day: "numeric", month: "long", year: "numeric" })}</small>
      </div>

      <div className="timer-form">
        <Segmented
          value={kind}
          onChange={setKind}
          options={[
            { value: "countdown", label: "Таймер" },
            { value: "stopwatch", label: "Секундомер" },
          ]}
        />
        <div className="row gap">
          <input className="input grow" placeholder="Название" value={label} maxLength={60} onChange={(e) => setLabel(e.target.value)} />
          {kind === "countdown" && (
            <input className="input" style={{ width: 80 }} placeholder="5:00" value={duration} onChange={(e) => setDuration(e.target.value)} onKeyDown={(e) => e.key === "Enter" && create()} />
          )}
          <IconButton title="Добавить" onClick={() => create()}>
            <Plus size={16} />
          </IconButton>
        </div>
        {kind === "countdown" && (
          <div className="row gap">
            {PRESETS.map((m) => (
              <button key={m} type="button" className="chip" onClick={() => create(m * 60_000)}>
                {m} мин
              </button>
            ))}
          </div>
        )}
      </div>
      {error && <Note kind="error">{error}</Note>}

      <div className="list scroll" style={{ maxHeight: 220 }}>
        {timers.length === 0 && <div className="empty small">Нет таймеров</div>}
        {timers.map((t) => (
          <TimerRow key={t.id} t={t} now={now} />
        ))}
      </div>
    </div>
  );
}

function TimerRow({ t, now }: { t: Timer; now: number }) {
  const countdown = t.kind === "countdown";
  const value = countdown ? timerRemaining(t, now) : timerElapsed(t, now);
  const running = t.state === "running";
  const pct = countdown && t.durationMs ? Math.min(100, (timerElapsed(t, now) / t.durationMs) * 100) : 0;
  return (
    <div className={`list-item timer-row state-${t.state}`}>
      {countdown ? <Hourglass size={16} /> : <TimerIcon size={16} />}
      <div className="grow">
        <div className="small muted">{t.label}</div>
        <div className="mono timer-value">{countdown ? formatClock(value) : formatStopwatch(value)}</div>
        {countdown && <div className="bar"><i style={{ width: `${pct}%` }} /></div>}
      </div>
      {t.state === "finished" && <span className="small accent">Готово</span>}
      <IconButton size="sm" title={running ? "Пауза" : "Старт"} onClick={() => ipc.timerCommand(t.id, running ? "pause" : "start")}>
        {running ? <Pause size={14} /> : <Play size={14} />}
      </IconButton>
      <IconButton size="sm" title="Сбросить" onClick={() => ipc.timerCommand(t.id, "reset")}>
        <RotateCcw size={14} />
      </IconButton>
      <IconButton size="sm" title="Удалить" onClick={() => ipc.timerCommand(t.id, "delete")}>
        <Trash2 size={14} />
      </IconButton>
    </div>
  );
}

