// Громкость отдельных программ и Bluetooth-наушники (заряд, подключение).

import { useState } from "react";
import { BatteryMedium, ChevronRight, Headphones, Volume2, VolumeX } from "lucide-react";
import { useLoad, usePoll } from "../hooks/common";
import { errorMessage, ipc } from "../services/ipc";
import type { AppVolume, Headphone } from "../types";
import { Slider, Spinner } from "./ui";

export function AppMixer({ onError }: { onError: (e: string) => void }) {
  const [open, setOpen] = useState(false);
  const apps = useLoad<AppVolume[]>(() => (open ? ipc.appVolumes() : Promise.resolve([])), [open]);
  usePoll(() => apps.reload(), 3000, open);
  const list = apps.data ?? [];
  return (
    <div className="mixer">
      <button type="button" className="mixer-head" aria-expanded={open} onClick={() => setOpen(!open)}>
        <span className="grow">Громкость программ</span>
        <ChevronRight size={14} className={`chev ${open ? "open" : ""}`} />
      </button>
      {open &&
        (list.length === 0 ? (
          <div className="small muted mixer-empty">Сейчас ни одна программа не играет звук</div>
        ) : (
          list.map((a) => (
            <div className="slider-row" key={a.id}>
              <button
                type="button"
                className="icon-btn sm"
                title={a.muted ? "Включить звук" : "Выключить звук"}
                onClick={() => ipc.appVolumeSet(a.id, null, !a.muted).then(apps.reload).catch((e) => onError(errorMessage(e)))}
              >
                {a.muted ? <VolumeX size={14} /> : <Volume2 size={14} />}
              </button>
              <span className="mixer-name ellipsis" title={a.name}>
                {a.name}
              </span>
              <Slider
                label={`Громкость: ${a.name}`}
                value={Math.round(Math.min(1, a.level) * 100)}
                onChange={(v) => apps.setData(list.map((x) => (x.id === a.id ? { ...x, level: v / 100 } : x)))}
                onCommit={(v) => ipc.appVolumeSet(a.id, v / 100, null).catch((e) => onError(errorMessage(e)))}
              />
            </div>
          ))
        ))}
    </div>
  );
}

export function HeadphonesRow({ onError }: { onError: (e: string) => void }) {
  const list = useLoad<Headphone[]>(() => ipc.headphones());
  usePoll(() => list.reload(), 15_000);
  const [busy, setBusy] = useState<string | null>(null);
  const items = list.data ?? [];
  if (!items.length) return null;
  return (
    <div className="phones">
      {items.map((h) => (
        <button
          key={h.id}
          type="button"
          className={`phone-chip ${h.connected ? "on" : ""}`}
          title={h.connected ? "Отключить" : "Подключить"}
          disabled={busy === h.id}
          onClick={() => {
            setBusy(h.id);
            ipc
              .headphoneConnect(h.id, !h.connected)
              .then(() => setTimeout(list.reload, 2500))
              .catch((e) => onError(errorMessage(e)))
              .finally(() => setBusy(null));
          }}
        >
          {busy === h.id ? <Spinner size={13} /> : <Headphones size={14} />}
          <span className="ellipsis">{h.name}</span>
          {h.battery != null && (
            <span className="phone-batt mono">
              <BatteryMedium size={13} />
              {h.battery}%
            </span>
          )}
        </button>
      ))}
    </div>
  );
}
