// Строительные блоки страниц настроек.

import { useState, type ReactNode } from "react";
import { Plus, X } from "lucide-react";
import { Button, Confirm, IconButton } from "../components/ui";

export function Page({ title, description, children, actions }: { title: string; description?: string; children: ReactNode; actions?: ReactNode }) {
  return (
    <div className="page">
      <div className="page-head">
        <div>
          <h1>{title}</h1>
          {description && <p className="muted">{description}</p>}
        </div>
        {actions && <div className="row gap">{actions}</div>}
      </div>
      {children}
    </div>
  );
}

export function Group({ title, children }: { title?: string; children: ReactNode }) {
  return (
    <section className="group">
      {title && <h2>{title}</h2>}
      <div className="group-body">{children}</div>
    </section>
  );
}

export function Field({ label, description, children, wide }: { label: ReactNode; description?: ReactNode; children?: ReactNode; wide?: boolean }) {
  return (
    <div className={`field ${wide ? "wide" : ""}`}>
      <div className="field-text">
        <div className="field-label">{label}</div>
        {description && <div className="field-desc">{description}</div>}
      </div>
      {children && <div className="field-control">{children}</div>}
    </div>
  );
}

export function NumberInput({ value, min, max, step = 1, onChange, suffix }: { value: number; min: number; max: number; step?: number; onChange: (v: number) => void; suffix?: string }) {
  const [local, setLocal] = useState(String(value));
  const commit = () => {
    const n = Number(local);
    if (Number.isFinite(n)) {
      const c = Math.min(max, Math.max(min, n));
      setLocal(String(c));
      if (c !== value) onChange(c);
    } else setLocal(String(value));
  };
  return (
    <span className="row gap">
      <input className="input" style={{ width: 90 }} type="number" min={min} max={max} step={step} value={local} onChange={(e) => setLocal(e.target.value)} onBlur={commit} onKeyDown={(e) => e.key === "Enter" && commit()} />
      {suffix && <span className="muted small">{suffix}</span>}
    </span>
  );
}

/** Кнопка опасного действия с обязательным подтверждением. */
export function ConfirmButton({ label, title, text, onConfirm, small }: { label: ReactNode; title: string; text: string; onConfirm: () => Promise<unknown> | void; small?: boolean }) {
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  return (
    <>
      <Button variant="danger" small={small} onClick={() => setOpen(true)}>
        {label}
      </Button>
      {open && (
        <>
          <div className="backdrop" onClick={() => !busy && setOpen(false)} />
          <Confirm
            title={title}
            text={text}
            confirmLabel="Подтвердить"
            danger
            busy={busy}
            onCancel={() => setOpen(false)}
            onConfirm={async () => {
              setBusy(true);
              try {
                await onConfirm();
              } finally {
                setBusy(false);
                setOpen(false);
              }
            }}
          />
        </>
      )}
    </>
  );
}

/** Редактор списка строк (исключения, игры и т.д.). */
export function ListEditor({ items, onChange, placeholder, extra }: { items: string[]; onChange: (v: string[]) => void; placeholder: string; extra?: ReactNode }) {
  const [draft, setDraft] = useState("");
  const add = () => {
    const v = draft.trim();
    if (!v || items.includes(v)) return;
    onChange([...items, v]);
    setDraft("");
  };
  return (
    <div className="list-editor">
      <div className="row gap">
        <input className="input grow" value={draft} placeholder={placeholder} onChange={(e) => setDraft(e.target.value)} onKeyDown={(e) => e.key === "Enter" && add()} />
        <IconButton title="Добавить" onClick={add}>
          <Plus size={16} />
        </IconButton>
        {extra}
      </div>
      {items.length > 0 && (
        <div className="tags">
          {items.map((i) => (
            <span key={i} className="tag">
              {i}
              <button type="button" aria-label={`Удалить ${i}`} onClick={() => onChange(items.filter((x) => x !== i))}>
                <X size={11} />
              </button>
            </span>
          ))}
        </div>
      )}
    </div>
  );
}
