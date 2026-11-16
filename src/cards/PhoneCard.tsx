// Телефон в острове: заряд, «Найти», буфер и файлы на телефон (в том числе
// перетаскиванием на карточку), музыка с телефона и SMS-переписка.

import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { ArrowLeft, BatteryCharging, BatteryMedium, ClipboardCopy, FileUp, MessageSquare, Music2, Pause, Play, Send, Settings, SkipBack, SkipForward, Smartphone, Volume2 } from "lucide-react";
import { IconButton, Note, Spinner } from "../components/ui";
import { useEvent, useLoad } from "../hooks/common";
import { errorMessage, ipc } from "../services/ipc";
import { useApp } from "../stores/app";
import { useIsland } from "../stores/island";
import { useMedia } from "../stores/media";
import type { PhoneStatus, Sms, SmsThread } from "../types";
import { formatRelative, formatTime } from "../utils/format";

function PhoneMusic() {
  const sessions = useMedia((s) => s.sessions).filter((s) => s.id.startsWith("phone:"));
  const artwork = useMedia((s) => s.artwork);
  const loadArtwork = useMedia((s) => s.loadArtwork);
  const s = sessions.find((x) => x.status === "playing") ?? sessions[0];
  useEffect(() => {
    if (s) loadArtwork(s);
  }, [s?.artworkKey]);
  if (!s) return null;
  const art = s.artworkKey ? artwork[s.artworkKey] : null;
  const ctl = (action: Parameters<typeof ipc.mediaControl>[1]) => ipc.mediaControl(s.id, action).catch(() => {});
  return (
    <div className="phone-music">
      {art ? <img className="phone-art" src={art} alt="" /> : <span className="phone-art empty"><Music2 size={16} /></span>}
      <div className="grow phone-track">
        <b>{s.title}</b>
        <span>{s.artist || s.appName}</span>
      </div>
      <IconButton size="sm" title="Предыдущий" disabled={!s.canPrev} onClick={() => ctl({ type: "previous" })}>
        <SkipBack size={14} />
      </IconButton>
      <IconButton size="sm" title={s.status === "playing" ? "Пауза" : "Воспроизвести"} onClick={() => ctl({ type: "playPause" })}>
        {s.status === "playing" ? <Pause size={15} /> : <Play size={15} />}
      </IconButton>
      <IconButton size="sm" title="Следующий" disabled={!s.canNext} onClick={() => ctl({ type: "next" })}>
        <SkipForward size={14} />
      </IconButton>
    </div>
  );
}

function Thread({ t, onBack }: { t: SmsThread; onBack: () => void }) {
  const msgs = useLoad<Sms[]>(() => ipc.phoneSmsThread(t.thread), [t.thread]);
  useEvent("phone://sms", () => ipc.phoneSmsThread(t.thread).then(msgs.setData).catch(() => {}));
  const [text, setText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const list = useRef<HTMLDivElement>(null);
  useEffect(() => {
    list.current?.scrollTo({ top: list.current.scrollHeight });
  }, [msgs.data?.length]);
  const send = () => {
    if (!text.trim() || busy) return;
    setBusy(true);
    setError(null);
    ipc
      .phoneSmsSend(t.address, text)
      .then(() => setText(""))
      .catch((e) => setError(errorMessage(e)))
      .finally(() => setBusy(false));
  };
  return (
    <div className="sms-thread">
      <div className="row gap">
        <IconButton size="sm" title="Назад" onClick={onBack}>
          <ArrowLeft size={15} />
        </IconButton>
        <b className="grow ellipsis">{t.name}</b>
      </div>
      <div className="sms-list scroll" ref={list}>
        {!msgs.data && <Spinner size={16} />}
        {msgs.data?.map((m) => (
          <div key={m.id} className={`sms-bubble ${m.outgoing ? "out" : "in"}`}>
            {m.body}
            <small>{formatTime(m.date)}</small>
          </div>
        ))}
      </div>
      <div className="row gap">
        <input
          className="input grow"
          placeholder="Сообщение"
          value={text}
          maxLength={2000}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") send();
          }}
        />
        <IconButton title="Отправить" disabled={busy || !text.trim()} onClick={send}>
          <Send size={15} />
        </IconButton>
      </div>
      {error && <Note kind="error">{error}</Note>}
    </div>
  );
}

export default function PhoneCard() {
  const status = useLoad<PhoneStatus>(() => ipc.phoneStatus());
  useEvent<PhoneStatus>("phone://status", (s) => status.setData(s));
  const smsOn = useApp((s) => s.settings?.phone.sms ?? true);
  const threads = useLoad<SmsThread[]>(() => (smsOn ? ipc.phoneSmsThreads() : Promise.resolve([])), [smsOn]);
  useEvent("phone://sms", () => smsOn && ipc.phoneSmsThreads().then(threads.setData).catch(() => {}));
  const [open, setOpen] = useState<SmsThread | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState<string | null>(null);
  const dragging = useIsland((s) => s.dragging);

  const dev = status.data?.devices.find((d) => d.paired && d.reachable) ?? status.data?.devices.find((d) => d.paired);
  const online = !!dev?.reachable;
  const run = (fn: () => Promise<unknown>, ok?: string) => {
    setError(null);
    setDone(null);
    fn()
      .then(() => ok && setDone(ok))
      .catch((e) => setError(errorMessage(e)));
  };

  // Файлы, брошенные на остров, пока открыта эта карточка, уходят на телефон.
  useEffect(() => {
    const un = listen<{ state: string; added?: number }>("shelf://drag", async (e) => {
      const st = useIsland.getState();
      if (e.payload.state !== "drop" || st.activeCard !== "phone" || !e.payload.added) return;
      const items = (await ipc.shelf()).filter((i) => !i.isDir).sort((a, b) => b.addedAt - a.addedAt).slice(0, e.payload.added);
      run(async () => {
        for (const it of items) await ipc.phoneSendFile(it.path);
      }, items.length === 1 ? "Файл отправлен" : `Отправлено файлов: ${items.length}`);
    });
    return () => {
      un.then((u) => u());
    };
  }, []);

  if (status.data && !status.data.enabled) {
    return (
      <div className="card-pad phone-empty">
        <Smartphone size={22} />
        <span>Связь с телефоном выключена</span>
        <button type="button" className="btn" onClick={() => ipc.openSettings("phone").catch(() => {})}>
          <Settings size={14} /> Настройки
        </button>
      </div>
    );
  }
  if (status.data && !dev) {
    return (
      <div className="card-pad phone-empty">
        <Smartphone size={22} />
        <span>Телефон не подключён. Установите KDE Connect на телефон и выберите этот компьютер.</span>
        <button type="button" className="btn" onClick={() => ipc.openSettings("phone").catch(() => {})}>
          <Settings size={14} /> Подключить
        </button>
      </div>
    );
  }
  if (open) {
    return (
      <div className="card-pad">
        <Thread t={open} onBack={() => setOpen(null)} />
      </div>
    );
  }
  return (
    <div className={`card-pad phone-card ${dragging ? "drop-on" : ""}`}>
      <div className="row gap phone-head">
        <Smartphone size={18} />
        <b className="grow ellipsis">{dev?.name ?? "Телефон"}</b>
        {online ? (
          dev?.battery != null && (
            <span className="phone-batt">
              {dev.charging ? <BatteryCharging size={15} /> : <BatteryMedium size={15} />} {dev.battery}%
            </span>
          )
        ) : (
          <span className="muted small">не в сети</span>
        )}
      </div>
      <div className="phone-actions">
        <button type="button" className="btn" disabled={!online} onClick={() => dev && run(() => ipc.phoneRing(dev.id), "Телефон звонит")}>
          <Volume2 size={14} /> Найти
        </button>
        <button type="button" className="btn" disabled={!online} onClick={() => run(() => ipc.phoneSendClipboard(), "Буфер отправлен")}>
          <ClipboardCopy size={14} /> Буфер
        </button>
        <button type="button" className="btn" disabled={!online} onClick={() => run(async () => { const n = await ipc.phonePickSend(); if (!n) throw new Error(""); }, "Отправлено")}>
          <FileUp size={14} /> Файл
        </button>
      </div>
      {dragging && <div className="phone-drop">Отпустите — файлы уйдут на телефон</div>}
      {done && <div className="small muted">{done}</div>}
      {error && <Note kind="error">{error}</Note>}
      <PhoneMusic />
      {smsOn && (
        <div className="sms-threads">
          <div className="row gap small muted">
            <MessageSquare size={13} /> Сообщения
          </div>
          {threads.data?.length === 0 && <div className="small muted">Нет сообщений. Разрешите KDE Connect на телефоне доступ к SMS.</div>}
          {threads.data?.slice(0, 8).map((t) => (
            <button type="button" key={t.thread} className={`list-item sms-item ${t.unread ? "unread" : ""}`} onClick={() => setOpen(t)}>
              <div className="grow">
                <div className="row between">
                  <b className="ellipsis">{t.name}</b>
                  <small className="muted">{formatRelative(t.date)}</small>
                </div>
                <div className="small muted ellipsis">
                  {t.outgoing ? "Вы: " : ""}
                  {t.last}
                </div>
              </div>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
