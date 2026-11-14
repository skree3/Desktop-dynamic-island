// Быстрые настройки: Wi-Fi, Bluetooth, режим «в самолёте», энергосбережение,
// батарея, громкость, яркость, переход в настройки ОС, выключение/перезагрузка.

import { useState, type ReactNode } from "react";
import {
  BatteryCharging,
  BatteryFull,
  BatteryLow,
  BatteryMedium,
  Bluetooth,
  BluetoothOff,
  Droplet,
  Leaf,
  Plane,
  Power,
  RotateCw,
  Settings,
  ShieldCheck,
  ShieldOff,
  Sun,
  Volume2,
  VolumeX,
  Wifi,
  WifiOff,
} from "lucide-react";
import { Confirm, IconButton, Note, Slider, Spinner } from "../components/ui";
import { AppMixer, HeadphonesRow } from "../components/SoundExtras";
import { useLoad, usePoll } from "../hooks/common";
import { errorMessage, ipc } from "../services/ipc";
import { useApp } from "../stores/app";
import type { QuickState } from "../types";
import { hexToRgba } from "../utils/format";

function Tile({ icon, label, sub, on, disabled, onClick, title }: { icon: ReactNode; label: string; sub?: string; on?: boolean; disabled?: boolean; onClick?: () => void; title?: string }) {
  return (
    <button type="button" className={`tile ${on ? "on" : ""}`} disabled={disabled} onClick={onClick} title={title}>
      <span className="tile-icon">{icon}</span>
      <span className="tile-label">{label}</span>
      {sub && <span className="tile-sub">{sub}</span>}
    </button>
  );
}

/** Непрозрачность панели — прямо в центре управления, с живым предпросмотром. */
function OpacityRow() {
  const opacity = useApp((s) => s.settings?.appearance.opacity ?? 1);
  const [live, setLive] = useState(Math.round(opacity * 100));
  const preview = (v: number) => {
    setLive(v);
    const bg = getComputedStyle(document.documentElement).getPropertyValue("--bg").trim() || "#000000";
    document.documentElement.style.setProperty("--island-bg", hexToRgba(bg, v / 100));
  };
  return (
    <div className="slider-row">
      <Droplet size={16} />
      <Slider label="Непрозрачность панели" value={Math.round(opacity * 100)} min={35} max={100} onChange={preview} onCommit={(v) => {
        preview(v);
        useApp.getState().update("appearance", { opacity: v / 100 }).catch(() => {});
      }} />
      <span className="small mono value" title="Непрозрачность панели">{live}</span>
    </div>
  );
}

export default function QuickCard() {
  const modules = useApp((s) => s.settings?.modules);
  const q = useLoad(() => ipc.quick());
  const bright = useLoad(() => (modules?.brightness ? ipc.brightness() : Promise.resolve(null)), [modules?.brightness]);
  const vpnClient = useApp((s) => s.settings?.vpn.client ?? "auto");
  const vpn = useLoad(() => (vpnClient === "none" ? Promise.resolve(null) : ipc.vpnStatus()), [vpnClient]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);
  const [confirm, setConfirm] = useState<null | { action: "shutdown" | "reboot"; token: string }>(null);

  // Пока карточка открыта — обновляем состояние раз в 5 секунд (изменения извне).
  usePoll(() => q.reload(), 5000, !!q.data);
  usePoll(() => vpn.reload(), 2500, !!vpn.data?.client);

  const act = async (key: string, fn: () => Promise<unknown>) => {
    setBusy(key);
    setError(null);
    try {
      await fn();
      await q.reload();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(null);
    }
  };

  if (q.loading && !q.data) return <div className="card-pad center"><Spinner /></div>;
  if (q.error && !q.data) return <div className="card-pad"><Note kind="error">{q.error}</Note></div>;
  const s = q.data as QuickState;
  const p = s.power;
  const BatteryIcon = p?.charging ? BatteryCharging : (p?.percent ?? 100) > 70 ? BatteryFull : (p?.percent ?? 100) > 30 ? BatteryMedium : BatteryLow;

  return (
    <div className="card-pad quick">
      <div className="tiles">
        <Tile
          icon={busy === "wifi" ? <Spinner /> : s.wifi.on ? <Wifi size={18} /> : <WifiOff size={18} />}
          label="Wi-Fi"
          sub={!s.wifi.available ? "Недоступно" : s.wifi.connected ? (s.wifi.ssid ?? "Подключено") : s.wifi.on ? "Не подключено" : "Выкл."}
          on={s.wifi.on}
          disabled={!s.wifi.canToggle || !!busy}
          title={s.wifi.error ?? s.wifi.ssid ?? undefined}
          onClick={() => act("wifi", () => ipc.wifiSet(!s.wifi.on))}
        />
        {modules?.bluetooth && (
          <Tile
            icon={busy === "bt" ? <Spinner /> : s.bluetooth.powered ? <Bluetooth size={18} /> : <BluetoothOff size={18} />}
            label="Bluetooth"
            sub={!s.bluetooth.available ? "Недоступно" : !s.bluetooth.powered ? "Выкл." : s.bluetooth.devices.length ? s.bluetooth.devices.map((d) => d.name).join(", ") : "Вкл., нет подключений"}
            on={s.bluetooth.powered}
            disabled={!s.bluetooth.canToggle || !!busy}
            title={s.bluetooth.error ?? (s.bluetooth.devices.map((d) => d.name).join(", ") || undefined)}
            onClick={() => act("bt", () => ipc.bluetoothSetPowered(!s.bluetooth.powered))}
          />
        )}
        <Tile
          icon={busy === "air" ? <Spinner /> : <Plane size={18} />}
          label="В самолёте"
          sub={s.airplane.known ? (s.airplane.on ? "Вкл." : "Выкл.") : "Недоступно"}
          on={s.airplane.on}
          disabled={!!busy}
          title={s.airplane.note}
          onClick={() => act("air", () => (s.airplane.canToggle ? ipc.airplaneSet(!s.airplane.on) : ipc.openSystemSettings("airplane")))}
        />
        <Tile
          icon={busy === "saver" ? <Spinner /> : <Leaf size={18} />}
          label="Энергосбережение"
          sub={p?.saver == null ? "Недоступно" : p.saver ? "Вкл." : "Выкл."}
          on={!!p?.saver}
          title={p?.saverCanToggle ? p.saverBackend ?? undefined : "Переключение — в параметрах системы"}
          onClick={() => (p?.saverCanToggle ? act("saver", () => ipc.energySaverSet(!p.saver)) : act("saver", () => ipc.openSystemSettings("battery")))}
        />
        <Tile
          icon={<BatteryIcon size={18} />}
          label="Батарея"
          sub={p?.hasBattery ? `${p.percent ?? "—"}%${p.charging ? " · заряд" : ""}` : "Нет батареи"}
          on={!!p?.charging}
          onClick={() => act("bat", () => ipc.openSystemSettings("power"))}
        />
        {vpn.data?.client ? (
          <Tile
            icon={busy === "vpn" ? <Spinner /> : vpn.data.connected ? <ShieldCheck size={18} /> : <ShieldOff size={18} />}
            label={`VPN · ${vpn.data.name}`}
            sub={!vpn.data.installed ? "Не найден" : vpn.data.connected ? "Подключён" : vpn.data.canControl ? "Отключён" : "Открыть"}
            on={vpn.data.connected}
            disabled={!!busy}
            title={vpn.data.canControl ? "Подключение к серверу, выбранному в " + vpn.data.name : "Открывает " + vpn.data.name}
            onClick={() =>
              !vpn.data!.installed
                ? ipc.openSettings("vpn")
                : act("vpn", async () => {
                    await ipc.vpnSet(!vpn.data!.connected);
                    // Клиенту нужно пару секунд, чтобы поднять или убрать туннель.
                    for (let i = 0; i < 8; i++) {
                      await new Promise((r) => setTimeout(r, 600));
                      const v = await vpn.reload();
                      if (v && v.connected !== vpn.data!.connected) break;
                    }
                  })
            }
          />
        ) : (
          <Tile icon={<Settings size={18} />} label="Параметры" sub="Системы" onClick={() => act("set", () => ipc.openSystemSettings("main"))} />
        )}
        {vpn.data?.client && <Tile icon={<Settings size={18} />} label="Параметры" sub="Системы" onClick={() => act("set", () => ipc.openSystemSettings("main"))} />}
      </div>

      {modules?.volume && (
        <div className="slider-row">
          {s.volume ? (
            <>
              <IconButton
                size="sm"
                title={s.volume.muted ? "Включить звук" : "Выключить звук"}
                onClick={() => {
                  const muted = !s.volume!.muted;
                  q.setData({ ...s, volume: { ...s.volume!, muted } });
                  ipc.volumeMute(muted).then(() => q.reload()).catch((e) => setError(errorMessage(e)));
                }}
              >
                {s.volume.muted || s.volume.level === 0 ? <VolumeX size={16} /> : <Volume2 size={16} />}
              </IconButton>
              <Slider
                label="Громкость"
                value={Math.round(s.volume.level * 100)}
                onChange={(v) => q.setData({ ...s, volume: { ...s.volume!, level: v / 100 } })}
                onCommit={(v) => {
                  q.setData({ ...s, volume: { ...s.volume!, level: v / 100 } });
                  ipc.volumeSet(v / 100).then(() => q.reload()).catch((e) => setError(errorMessage(e)));
                }}
              />
              <span className="small mono value">{Math.round(s.volume.level * 100)}</span>
            </>
          ) : (
            <span className="small muted">Громкость: {s.volumeError ?? "недоступно"}</span>
          )}
        </div>
      )}
      {modules?.volume && s.volume && (
        <div className="row gap small muted sub-row">
          <span className="ellipsis grow">Вывод: {s.volume.device?.name ?? "—"}</span>
          {s.volume.canSelectDevice && s.volume.devices.length > 1 ? (
            <select className="input select-sm" value={s.volume.device?.id ?? ""} onChange={(e) => act("dev", () => ipc.volumeSetDevice(e.target.value))}>
              {s.volume.devices.map((d) => (
                <option key={d.id} value={d.id}>
                  {d.name}
                </option>
              ))}
            </select>
          ) : (
            <button type="button" className="link" onClick={() => ipc.openSystemSettings("sound")}>
              Сменить в параметрах
            </button>
          )}
        </div>
      )}
      {modules?.volume && <HeadphonesRow onError={setError} />}
      {modules?.volume && <AppMixer onError={setError} />}

      {modules?.brightness && (
        <>
          {bright.data?.displays.map((d) => {
            const setValue = (v: number) =>
              bright.setData({ ...bright.data!, displays: bright.data!.displays.map((x) => (x.id === d.id ? { ...x, value: v } : x)) });
            return (
              <div className="slider-row" key={d.id}>
                <Sun size={16} />
                <Slider
                  label={`Яркость: ${d.name}`}
                  value={d.value}
                  onChange={setValue}
                  onCommit={(v) => {
                    setValue(v);
                    ipc.brightnessSet(d.id, v).then(() => bright.reload()).catch((e) => setError(errorMessage(e)));
                  }}
                />
                <span className="small mono value" title={`${d.name} (${d.method})`}>{d.value}</span>
              </div>
            );
          })}
          {bright.data?.error && (
            <div className="row gap small muted sub-row">
              <Sun size={14} />
              <span className="grow">{bright.data.error}</span>
              <button type="button" className="link" onClick={() => ipc.openSystemSettings("display")}>
                Дисплей
              </button>
            </div>
          )}
          {bright.error && <div className="small muted">Яркость: {bright.error}</div>}
        </>
      )}

      <OpacityRow />

      {modules?.power && (
        <div className="power-row">
          <button type="button" className="btn btn-ghost" onClick={async () => setConfirm({ action: "reboot", token: await ipc.powerRequest("reboot") })}>
            <RotateCw size={15} /> Перезагрузить
          </button>
          <button type="button" className="btn btn-ghost danger-text" onClick={async () => setConfirm({ action: "shutdown", token: await ipc.powerRequest("shutdown") })}>
            <Power size={15} /> Выключить
          </button>
        </div>
      )}
      {confirm && (
        <Confirm
          title={confirm.action === "shutdown" ? "Выключить компьютер?" : "Перезагрузить компьютер?"}
          text="Несохранённые данные в открытых приложениях могут быть потеряны. Приложения смогут отменить завершение работы."
          confirmLabel={confirm.action === "shutdown" ? "Выключить" : "Перезагрузить"}
          danger
          busy={busy === "power"}
          onCancel={() => setConfirm(null)}
          onConfirm={() =>
            act("power", async () => {
              await ipc.powerExecute(confirm.action, confirm.token);
              setConfirm(null);
            })
          }
        />
      )}
      {error && <Note kind="error">{error}</Note>}
    </div>
  );
}
