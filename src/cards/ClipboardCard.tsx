import { useEffect, useState } from "react";
import { Copy, FileText, Files, Image as ImageIcon, Pin, PinOff, Search, Trash2, X } from "lucide-react";
import { Confirm, IconButton, Note } from "../components/ui";
import { useEvent, useLoad, useNow } from "../hooks/common";
import { errorMessage, events, ipc } from "../services/ipc";
import { useApp } from "../stores/app";
import type { ClipItem } from "../types";
import { formatRelative } from "../utils/format";

function Thumb({ id }: { id: number }) {
  const [src, setSrc] = useState<string | null>(null);
  useEffect(() => {
    let alive = true;
    ipc.clipboardImage(id).then((s) => alive && setSrc(s)).catch(() => {});
    return () => {
      alive = false;
    };
  }, [id]);
  return src ? <img className="clip-thumb" src={src} alt="" /> : <ImageIcon size={16} />;
}

function OwnHistory() {
  const enabled = useApp((s) => s.settings?.clipboard.historyEnabled);
  const env = useApp((s) => s.environment);
  const [query, setQuery] = useState("");
  const [copied, setCopied] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirmClear, setConfirmClear] = useState(false);
  const list = useLoad(() => ipc.clipboardList(query, 100), [query]);
  const now = useNow(30_000);
  useEvent(events.CLIPBOARD_CHANGED, () => list.reload());

  const copy = async (item: ClipItem) => {
    try {
      await ipc.clipboardCopy(item.id);
      setCopied(item.id);
      setTimeout(() => setCopied(null), 1200);
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  if (!enabled) {
    return (
      <div className="card-pad">
        <div className="empty">История буфера обмена отключена</div>
        <button type="button" className="btn" onClick={() => useApp.getState().update("clipboard", { historyEnabled: true })}>
          Включить
        </button>
      </div>
    );
  }
  const items = list.data ?? [];
  return (
    <div className="card-pad">
      <div className="search">
        <Search size={14} />
        <input className="input-bare" placeholder="Поиск в истории" value={query} onChange={(e) => setQuery(e.target.value)} />
        {query && (
          <IconButton size="sm" title="Очистить" onClick={() => setQuery("")}>
            <X size={13} />
          </IconButton>
        )}
        <IconButton size="sm" title="Очистить историю" onClick={() => setConfirmClear(true)}>
          <Trash2 size={14} />
        </IconButton>
      </div>
      {env?.sessionType === "wayland" && <div className="small muted">Wayland: фоновое чтение буфера может быть ограничено композитором.</div>}
      {error && <Note kind="error">{error}</Note>}
      <div className="list scroll" style={{ maxHeight: 320 }}>
        {items.length === 0 && <div className="empty small">{query ? "Ничего не найдено" : "История пуста"}</div>}
        {items.map((it) => (
          <div key={it.id} className="list-item clip" onDoubleClick={() => copy(it)} title="Двойной щелчок — скопировать">
            <span className="clip-kind">{it.kind === "image" ? <Thumb id={it.id} /> : it.kind === "files" ? <Files size={16} /> : <FileText size={16} />}</span>
            <div className="grow">
              <div className="clamp2">{it.preview}</div>
              <div className="small muted">
                {formatRelative(it.createdAt, now)}
                {it.sourceApp ? ` · ${it.sourceApp}` : ""}
              </div>
            </div>
            <IconButton size="sm" title={copied === it.id ? "Скопировано" : "Скопировать"} active={copied === it.id} onClick={() => copy(it)}>
              <Copy size={13} />
            </IconButton>
            <IconButton size="sm" title={it.pinned ? "Открепить" : "Закрепить"} active={it.pinned} onClick={() => ipc.clipboardPin(it.id, !it.pinned)}>
              {it.pinned ? <PinOff size={13} /> : <Pin size={13} />}
            </IconButton>
            <IconButton size="sm" title="Удалить" onClick={() => ipc.clipboardDelete(it.id)}>
              <X size={13} />
            </IconButton>
          </div>
        ))}
      </div>
      {confirmClear && (
        <Confirm
          title="Очистить историю?"
          text="Будут удалены все записи, кроме закреплённых. Действие нельзя отменить."
          confirmLabel="Очистить"
          danger
          onCancel={() => setConfirmClear(false)}
          onConfirm={() => ipc.clipboardClear(true).then(() => setConfirmClear(false)).catch((e) => setError(errorMessage(e)))}
        />
      )}
    </div>
  );
}

type WinItem = { id: string; kind: string; preview: string; createdAt: number; image: string | null };

/** Windows: показываем и очищаем тот же журнал, что открывается по Win+V. */
function WindowsHistory({ onFallback }: { onFallback: (reason: string) => void }) {
  const [query, setQuery] = useState("");
  const [copied, setCopied] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirmClear, setConfirmClear] = useState(false);
  const list = useLoad(() => ipc.clipboardSystemHistory(), []);
  const now = useNow(30_000);
  useEvent(events.CLIPBOARD_CHANGED, () => list.reload());

  useEffect(() => {
    // «Нет доступа» бывает, если окно не в фокусе — это не повод уходить в запасной режим.
    if (list.error && !list.error.includes("активн")) onFallback(list.error);
  }, [list.error]);

  const run = async (fn: () => Promise<unknown>) => {
    setError(null);
    try {
      await fn();
      await list.reload();
    } catch (e) {
      setError(errorMessage(e));
    }
  };
  const q = query.trim().toLowerCase();
  const items = (list.data ?? []).filter((i: WinItem) => !q || i.preview.toLowerCase().includes(q));
  return (
    <div className="card-pad">
      <div className="search">
        <Search size={14} />
        <input className="input-bare" placeholder="Поиск в журнале Win+V" value={query} onChange={(e) => setQuery(e.target.value)} />
        {query && (
          <IconButton size="sm" title="Очистить поиск" onClick={() => setQuery("")}>
            <X size={13} />
          </IconButton>
        )}
        <IconButton size="sm" title="Очистить журнал буфера обмена (Win+V)" onClick={() => setConfirmClear(true)}>
          <Trash2 size={14} />
        </IconButton>
      </div>
      {error && <Note kind="error">{error}</Note>}
      <div className="list scroll" style={{ maxHeight: 320 }}>
        {list.loading && !list.data && <div className="empty small">Загрузка…</div>}
        {list.error && list.error.includes("активн") && (
          <Note kind="warn">
            {list.error}{" "}
            <button type="button" className="link" onClick={() => list.reload()}>
              Обновить
            </button>
          </Note>
        )}
        {list.data && items.length === 0 && <div className="empty small">{query ? "Ничего не найдено" : "Журнал Win+V пуст"}</div>}
        {items.map((it: WinItem) => (
          <div key={it.id} className="list-item clip" onDoubleClick={() => run(() => ipc.clipboardSystemItem(it.id, "copy"))} title="Двойной щелчок — вставить в буфер">
            <span className="clip-kind">
              {it.image ? <img className="clip-thumb" src={it.image} alt="" /> : it.kind === "image" ? <ImageIcon size={16} /> : it.kind === "files" ? <Files size={16} /> : <FileText size={16} />}
            </span>
            <div className="grow">
              <div className="clamp2">{it.preview}</div>
              <div className="small muted">{formatRelative(it.createdAt, now)}</div>
            </div>
            <IconButton
              size="sm"
              title={copied === it.id ? "Скопировано" : "Скопировать"}
              active={copied === it.id}
              onClick={() =>
                run(async () => {
                  await ipc.clipboardSystemItem(it.id, "copy");
                  setCopied(it.id);
                  setTimeout(() => setCopied(null), 1200);
                })
              }
            >
              <Copy size={13} />
            </IconButton>
            <IconButton size="sm" title="Удалить из журнала" onClick={() => run(() => ipc.clipboardSystemItem(it.id, "delete"))}>
              <X size={13} />
            </IconButton>
          </div>
        ))}
      </div>
      {confirmClear && (
        <Confirm
          title="Очистить журнал буфера обмена?"
          text="Будет очищен журнал Windows (Win+V). Записи, закреплённые в самом Win+V, Windows сохраняет."
          confirmLabel="Очистить"
          danger
          onCancel={() => setConfirmClear(false)}
          onConfirm={() => run(() => ipc.clipboardSystemClear()).then(() => setConfirmClear(false))}
        />
      )}
    </div>
  );
}

export default function ClipboardCard() {
  const os = useApp((s) => s.os);
  const [fallback, setFallback] = useState<string | null>(null);
  if (os === "windows" && !fallback) return <WindowsHistory onFallback={setFallback} />;
  return (
    <>
      {fallback && (
        <div className="card-pad" style={{ paddingBottom: 0 }}>
          <Note kind="warn">{fallback}</Note>
        </div>
      )}
      <OwnHistory />
    </>
  );
}
