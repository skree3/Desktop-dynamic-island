import { useState } from "react";
import { Bluetooth, BluetoothConnected, RefreshCw, Settings } from "lucide-react";
import { IconButton, Note, Spinner, Toggle } from "../components/ui";
import { useLoad, usePoll } from "../hooks/common";
import { errorMessage, ipc } from "../services/ipc";

export default function BluetoothCard() {
  const bt = useLoad(() => ipc.bluetooth());
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  usePoll(() => bt.reload(), 8000, !!bt.data);

  if (bt.loading && !bt.data) return <div className="card-pad center"><Spinner /></div>;
  if (bt.error && !bt.data) return <div className="card-pad"><Note kind="error">{bt.error}</Note></div>;
  const s = bt.data!;
  const toggle = async (on: boolean) => {
    setBusy(true);
    setError(null);
    try {
      await ipc.bluetoothSetPowered(on);
      await bt.reload();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="card-pad">
      <div className="row between">
        <div className="row gap">
          <Bluetooth size={18} />
          <div>
            <div>Bluetooth</div>
            <div className="small muted">{!s.available ? "Адаптер не найден" : s.powered ? "Включён" : "Выключен"}</div>
          </div>
        </div>
        <div className="row gap">
          {busy && <Spinner size={14} />}
          <IconButton size="sm" title="Обновить" onClick={() => bt.reload()}>
            <RefreshCw size={14} />
          </IconButton>
          <IconButton size="sm" title="Параметры Bluetooth" onClick={() => ipc.openSystemSettings("bluetooth").catch((e) => setError(errorMessage(e)))}>
            <Settings size={14} />
          </IconButton>
          <Toggle checked={s.powered} disabled={!s.canToggle || busy} label="Bluetooth" onChange={toggle} />
        </div>
      </div>
      {s.error && <Note kind="warn">{s.error}</Note>}
      {error && <Note kind="error">{error}</Note>}
      {s.available && (
        <>
          <div className="section-title">Устройства</div>
          <div className="list scroll" style={{ maxHeight: 260 }}>
            {s.devices.length === 0 && <div className="small muted">Нет сопряжённых устройств</div>}
            {s.devices.map((d) => (
              <div key={d.id} className="list-item">
                {d.connected ? <BluetoothConnected size={16} className="accent" /> : <Bluetooth size={16} className="muted" />}
                <div className="grow">
                  <div className="ellipsis">{d.name}</div>
                  <div className="small muted">{d.connected ? "Подключено" : "Сопряжено"}{d.battery != null ? ` · ${d.battery}%` : ""}</div>
                </div>
              </div>
            ))}
          </div>
        </>
      )}
    </div>
  );
}
