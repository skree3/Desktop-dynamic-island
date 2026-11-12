// Базовые элементы интерфейса в стиле Windows 11 / Dynamic Island.

import { memo, useEffect, useRef, useState, type ReactNode } from "react";
import { AlertTriangle, Info, Loader2, Package } from "lucide-react";
import { ipc } from "../services/ipc";

export function Toggle({ checked, onChange, disabled, label }: { checked: boolean; onChange: (v: boolean) => void; disabled?: boolean; label?: string }) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      className={`toggle ${checked ? "on" : ""}`}
      disabled={disabled}
      onClick={(e) => {
        e.stopPropagation();
        onChange(!checked);
      }}
    >
      <span className="knob" />
    </button>
  );
}

export function Slider({
  value,
  min = 0,
  max = 100,
  step = 1,
  onChange,
  onCommit,
  disabled,
  label,
}: {
  value: number;
  min?: number;
  max?: number;
  step?: number;
  onChange?: (v: number) => void;
  onCommit?: (v: number) => void;
  disabled?: boolean;
  label?: string;
}) {
  const [local, setLocal] = useState(value);
  const [dragging, setDragging] = useState(false);
  const draggingRef = useRef(false);
  // Синхронизируемся только при изменении внешнего значения (например, громкость
  // изменили клавишами). Отпускание ползунка не возвращает старое значение.
  useEffect(() => {
    if (!draggingRef.current) setLocal(value);
  }, [value]);
  const pct = ((local - min) / (max - min || 1)) * 100;
  return (
    <input
      className="slider"
      type="range"
      aria-label={label}
      min={min}
      max={max}
      step={step}
      value={local}
      disabled={disabled}
      style={{ ["--pct" as any]: `${pct}%` }}
      onPointerDown={() => {
        draggingRef.current = true;
        setDragging(true);
      }}
      onChange={(e) => {
        const v = Number(e.target.value);
        setLocal(v);
        onChange?.(v);
        if (!dragging) onCommit?.(v);
      }}
      onPointerUp={(e) => {
        draggingRef.current = false;
        setDragging(false);
        onCommit?.(Number((e.target as HTMLInputElement).value));
      }}
      onKeyUp={(e) => onCommit?.(Number((e.target as HTMLInputElement).value))}
    />
  );
}

export function Button({
  children,
  onClick,
  variant = "default",
  disabled,
  title,
  small,
  type = "button",
}: {
  children: ReactNode;
  onClick?: () => void;
  variant?: "default" | "primary" | "danger" | "ghost";
  disabled?: boolean;
  title?: string;
  small?: boolean;
  type?: "button" | "submit";
}) {
  return (
    <button type={type} className={`btn btn-${variant} ${small ? "btn-sm" : ""}`} onClick={onClick} disabled={disabled} title={title}>
      {children}
    </button>
  );
}

export function IconButton({ children, onClick, title, active, disabled, size = "md" }: { children: ReactNode; onClick?: () => void; title: string; active?: boolean; disabled?: boolean; size?: "sm" | "md" | "lg" }) {
  return (
    <button
      type="button"
      className={`icon-btn icon-btn-${size} ${active ? "active" : ""}`}
      title={title}
      aria-label={title}
      disabled={disabled}
      onClick={(e) => {
        e.stopPropagation();
        onClick?.();
      }}
    >
      {children}
    </button>
  );
}

export function Note({ kind = "info", children }: { kind?: "info" | "warn" | "error"; children: ReactNode }) {
  return (
    <div className={`note note-${kind}`}>
      {kind === "info" ? <Info size={14} /> : <AlertTriangle size={14} />}
      <div>{children}</div>
    </div>
  );
}

export function Spinner({ size = 16 }: { size?: number }) {
  return <Loader2 size={size} className="spin" />;
}

export function Empty({ icon, children }: { icon?: ReactNode; children: ReactNode }) {
  return (
    <div className="empty">
      {icon}
      <span>{children}</span>
    </div>
  );
}

export function Segmented<T extends string>({ value, options, onChange }: { value: T; options: { value: T; label: string }[]; onChange: (v: T) => void }) {
  return (
    <div className="segmented" role="radiogroup">
      {options.map((o) => (
        <button key={o.value} type="button" role="radio" aria-checked={o.value === value} className={o.value === value ? "on" : ""} onClick={() => onChange(o.value)}>
          {o.label}
        </button>
      ))}
    </div>
  );
}

/** Мини-график без библиотек: SVG polyline. */
export const Sparkline = memo(function Sparkline({ data, max, color = "var(--accent)", height = 28 }: { data: number[]; max?: number; color?: string; height?: number }) {
  if (data.length < 2) return <svg className="spark" height={height} />;
  const m = max ?? Math.max(1, ...data);
  const w = 100;
  const pts = data.map((v, i) => `${(i / (data.length - 1)) * w},${height - (Math.min(v, m) / m) * (height - 2) - 1}`).join(" ");
  return (
    <svg className="spark" viewBox={`0 0 ${w} ${height}`} preserveAspectRatio="none" height={height}>
      <polyline points={`0,${height} ${pts} ${w},${height}`} fill={color} opacity={0.14} stroke="none" />
      <polyline points={pts} fill="none" stroke={color} strokeWidth={1.6} vectorEffect="non-scaling-stroke" />
    </svg>
  );
});

const iconCache = new Map<string, string | null>();

/** Иконка приложения, загружаемая лениво и кэшируемая. */
export const AppIcon = memo(function AppIcon({ id, size = 28, fallback }: { id: string; size?: number; fallback?: ReactNode }) {
  const [src, setSrc] = useState<string | null | undefined>(iconCache.get(id));
  useEffect(() => {
    if (iconCache.has(id)) {
      setSrc(iconCache.get(id));
      return;
    }
    let alive = true;
    ipc
      .appsIcon(id)
      .then((u) => {
        iconCache.set(id, u);
        if (alive) setSrc(u);
      })
      .catch(() => {
        iconCache.set(id, null);
        if (alive) setSrc(null);
      });
    return () => {
      alive = false;
    };
  }, [id]);
  if (src) return <img className="app-icon" src={src} width={size} height={size} alt="" draggable={false} />;
  return <span className="app-icon placeholder" style={{ width: size, height: size }}>{fallback ?? <Package size={size * 0.6} />}</span>;
});

export function Confirm({ title, text, confirmLabel, danger, onConfirm, onCancel, busy }: { title: string; text: string; confirmLabel: string; danger?: boolean; onConfirm: () => void; onCancel: () => void; busy?: boolean }) {
  return (
    <div className="confirm" role="alertdialog" aria-label={title} onClick={(e) => e.stopPropagation()}>
      <div className="confirm-title">{title}</div>
      <div className="confirm-text">{text}</div>
      <div className="confirm-actions">
        <Button onClick={onCancel} disabled={busy}>
          Отмена
        </Button>
        <Button variant={danger ? "danger" : "primary"} onClick={onConfirm} disabled={busy}>
          {busy ? <Spinner size={14} /> : confirmLabel}
        </Button>
      </div>
    </div>
  );
}
