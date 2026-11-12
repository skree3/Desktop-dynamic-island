import { useEffect } from "react";
import { ArrowDown, ArrowUp, Battery, Cpu, HardDrive, MemoryStick, MonitorSmartphone, Thermometer } from "lucide-react";
import { Note, Sparkline, Spinner } from "../components/ui";
import { ipc } from "../services/ipc";
import { useSys } from "../stores/sysmon";
import { formatBytes, formatPercent, formatSpeed, formatTemp } from "../utils/format";

function Metric({ icon, label, value, sub, data, max }: { icon: React.ReactNode; label: string; value: string; sub?: string; data?: number[]; max?: number }) {
  return (
    <div className="metric">
      <div className="metric-head">
        {icon}
        <span>{label}</span>
        <strong className="mono">{value}</strong>
      </div>
      {sub && <div className="metric-sub ellipsis" title={sub}>{sub}</div>}
      {data && <Sparkline data={data} max={max} />}
    </div>
  );
}

export default function SystemCard() {
  const snap = useSys((s) => s.snapshot);
  const cpu = useSys((s) => s.cpu);
  const ram = useSys((s) => s.ram);
  const gpu = useSys((s) => s.gpu);
  const rx = useSys((s) => s.rx);
  const tx = useSys((s) => s.tx);

  // Пока опрос стартует — разовый снимок.
  useEffect(() => {
    if (!useSys.getState().snapshot) ipc.sysmon().then((s) => s && useSys.getState().push(s, 60)).catch(() => {});
  }, []);

  if (!snap) return <div className="card-pad center"><Spinner /></div>;
  const ramPct = snap.ramTotal ? (snap.ramUsed / snap.ramTotal) * 100 : 0;
  const p = snap.power;
  return (
    <div className="card-pad">
      <div className="metrics">
        <Metric icon={<Cpu size={14} />} label="CPU" value={formatPercent(snap.cpu)} sub={`${snap.cpuName || "Процессор"} · ${snap.cores} потоков`} data={cpu} max={100} />
        <Metric icon={<MemoryStick size={14} />} label="RAM" value={formatPercent(ramPct)} sub={`${formatBytes(snap.ramUsed)} из ${formatBytes(snap.ramTotal)}`} data={ram} max={100} />
        <Metric
          icon={<MonitorSmartphone size={14} />}
          label="GPU"
          value={snap.gpu.available ? formatPercent(snap.gpu.usage) : "Недоступно"}
          sub={snap.gpu.available ? [snap.gpu.name, snap.gpu.memoryTotal ? `${formatBytes(snap.gpu.memoryUsed)} / ${formatBytes(snap.gpu.memoryTotal)}` : null, snap.gpu.backend].filter(Boolean).join(" · ") : "Нет поддерживаемого интерфейса (NVML / PDH / sysfs)"}
          data={snap.gpu.available ? gpu : undefined}
          max={100}
        />
        <Metric icon={<Thermometer size={14} />} label="Температура" value={`CPU ${formatTemp(snap.cpuTemp)}`} sub={`GPU ${formatTemp(snap.gpu.temperature)}`} />
        <Metric icon={<ArrowDown size={14} />} label="Сеть ↓" value={formatSpeed(snap.netRxBps)} data={rx} />
        <Metric icon={<ArrowUp size={14} />} label="Сеть ↑" value={formatSpeed(snap.netTxBps)} data={tx} />
      </div>
      <div className="section-title">Диски</div>
      <div className="list">
        {snap.disks.length === 0 && <div className="small muted">Недоступно</div>}
        {snap.disks.slice(0, 6).map((d) => {
          const used = d.total - d.available;
          const pct = d.total ? (used / d.total) * 100 : 0;
          return (
            <div key={d.mount} className="disk">
              <HardDrive size={14} />
              <span className="ellipsis" title={d.name}>{d.mount}</span>
              <div className="bar grow"><i style={{ width: `${pct}%` }} className={pct > 90 ? "danger" : ""} /></div>
              <span className="small mono">{formatBytes(d.available)} своб.</span>
            </div>
          );
        })}
      </div>
      <div className="row gap small muted" style={{ marginTop: 8 }}>
        <Battery size={14} />
        {p?.hasBattery ? (
          <span>
            {p.percent}% · {p.charging ? "заряжается" : p.onAc ? "от сети" : "от батареи"}
            {p.secsRemaining ? ` · ~${Math.round(p.secsRemaining / 60)} мин` : ""}
          </span>
        ) : (
          <span>{p ? `Батарея отсутствует · ${p.onAc === false ? "от батареи" : "питание от сети"}` : "Состояние питания недоступно"}</span>
        )}
      </div>
      {snap.cpuTemp == null && (
        <Note kind="info">
          Температура CPU недоступна: {navigator.userAgent.includes("Windows") ? "Windows не предоставляет датчики без драйвера или прав администратора." : "датчики hwmon не найдены (модуль ядра k10temp/coretemp)."}
        </Note>
      )}
    </div>
  );
}
