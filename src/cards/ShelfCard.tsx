// Полка: перетащите файлы на остров — они появятся здесь. Хранятся только ссылки.

import { useRef, useState } from "react";
import { Copy, ExternalLink, File, Folder, FolderOpen, Inbox, Languages, ScanText, Scissors, Smartphone, Trash2, X } from "lucide-react";
import { IconButton, Note } from "../components/ui";
import { useEvent, useLoad } from "../hooks/common";
import { errorMessage, ipc } from "../services/ipc";
import { useIsland } from "../stores/island";
import { formatBytes } from "../utils/format";

export default function ShelfCard() {
  const items = useLoad(() => ipc.shelf());
  const dragging = useIsland((s) => s.dragging);
  const [error, setError] = useState<string | null>(null);
  useEvent("shelf://changed", () => items.reload());
  // Кнопка «На телефон» — только если есть сопряжённый телефон в сети.
  const phone = useLoad(() => ipc.phoneStatus().catch(() => null));
  useEvent<import("../types").PhoneStatus>("phone://status", (st) => phone.setData(st));
  const phoneReady = !!phone.data?.devices.some((d) => d.paired && d.reachable);
  // Текст с картинки (OCR) и его перевод.
  const [ocr, setOcr] = useState<{ text: string; busy: boolean; translated?: string } | null>(null);
  const isImage = (name: string) => /\.(png|jpe?g|bmp|webp|gif)$/i.test(name);
  const recognize = (id: number) => {
    setOcr({ text: "", busy: true });
    ipc
      .ocrShelfItem(id)
      .then((text) => setOcr({ text: text || "Текст не найден", busy: false }))
      .catch((e) => {
        setOcr(null);
        setError(errorMessage(e));
      });
  };

  const run = (fn: () => Promise<unknown>) => {
    setError(null);
    fn().catch((e) => setError(errorMessage(e)));
  };
  const list = items.data ?? [];
  // Перетаскивание файла из полки в другое окно: начинаем после сдвига на 6 px.
  const drag = useRef<{ id: number; x: number; y: number; started: boolean } | null>(null);
  const onItemDown = (id: number, e: React.PointerEvent) => {
    if (e.button !== 0 || (e.target as HTMLElement).closest("button")) return;
    drag.current = { id, x: e.screenX, y: e.screenY, started: false };
    // Курсор быстро уходит за край окна острова — без захвата движения туда не доходят.
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  };
  const onItemMove = (e: React.PointerEvent) => {
    const d = drag.current;
    if (!d || d.started || (e.buttons & 1) === 0) return;
    if (Math.abs(e.screenX - d.x) + Math.abs(e.screenY - d.y) > 4) {
      d.started = true;
      ipc.shelfDragOut([d.id]).catch((err) => setError(errorMessage(err)));
    }
  };
  const onItemUp = () => {
    drag.current = null;
  };
  const shot = () => run(() => ipc.screenshotRegion());
  return (
    <div className="card-pad">
      {(dragging || (items.data && list.length === 0)) && (
        <div className={`shelf-drop ${dragging ? "over" : ""}`}>
          <Inbox size={26} />
          <span>{dragging ? "Отпустите, чтобы положить на полку" : "Перетащите файлы на остров — они сразу окажутся здесь"}</span>
          {!dragging && (
            <button type="button" className="chip" onClick={shot}>
              <Scissors size={12} /> Снимок области
            </button>
          )}
        </div>
      )}
      {list.length > 0 && (
        <>
          <div className="row between section-title" style={{ marginTop: 4 }}>
            <span>Файлы ({list.length})</span>
            <div className="row" style={{ gap: 2 }}>
              <IconButton size="sm" title="Снимок области экрана на полку" onClick={shot}>
                <Scissors size={13} />
              </IconButton>
              <IconButton size="sm" title="Скопировать все (вставить в проводнике или чате)" onClick={() => run(() => ipc.shelfCommand("copy", undefined, list.filter((i) => i.exists).map((i) => i.id)))}>
                <Copy size={13} />
              </IconButton>
              <IconButton size="sm" title="Очистить полку (файлы не удаляются)" onClick={() => run(() => ipc.shelfCommand("clear"))}>
                <Trash2 size={13} />
              </IconButton>
            </div>
          </div>
          <div className="list scroll" style={{ maxHeight: 280 }}>
            {list.map((it) => (
              <div
                key={it.id}
                className={`list-item ${it.exists ? "draggable" : "missing"}`}
                data-no-drag
                onPointerDown={(e) => it.exists && onItemDown(it.id, e)}
                onPointerMove={onItemMove}
                onPointerUp={onItemUp}
                onDoubleClick={() => it.exists && run(() => ipc.shelfCommand("open", it.id))}
                title={it.exists ? `${it.path}
Перетащите в чат или папку` : it.path}
              >
                {it.isDir ? <Folder size={16} className="accent" /> : <File size={16} />}
                <div className="grow">
                  <div className="ellipsis">{it.name}</div>
                  <div className="small muted">{it.exists ? (it.isDir ? "Папка" : formatBytes(it.size)) : "Файл перемещён или удалён"}</div>
                </div>
                {it.exists && (
                  <>
                    <IconButton size="sm" title="Открыть" onClick={() => run(() => ipc.shelfCommand("open", it.id))}>
                      <ExternalLink size={13} />
                    </IconButton>
                    <IconButton size="sm" title="Показать в папке" onClick={() => run(() => ipc.shelfCommand("reveal", it.id))}>
                      <FolderOpen size={13} />
                    </IconButton>
                    {isImage(it.name) && it.exists && (
                      <IconButton size="sm" title="Текст с картинки" onClick={() => recognize(it.id)}>
                        <ScanText size={13} />
                      </IconButton>
                    )}
                    {phoneReady && !it.isDir && (
                      <IconButton size="sm" title="Отправить на телефон" onClick={() => run(() => ipc.phoneSendFile(it.path))}>
                        <Smartphone size={13} />
                      </IconButton>
                    )}
                    <IconButton size="sm" title="Скопировать" onClick={() => run(() => ipc.shelfCommand("copy", it.id))}>
                      <Copy size={13} />
                    </IconButton>
                  </>
                )}
                <IconButton size="sm" title="Убрать с полки" onClick={() => run(() => ipc.shelfCommand("remove", it.id))}>
                  <X size={13} />
                </IconButton>
              </div>
            ))}
          </div>
        </>
      )}
      {ocr && (
        <div className="ocr-panel">
          {ocr.busy ? (
            <div className="small muted">Распознаю текст…</div>
          ) : (
            <>
              <div className="ocr-text scroll">{ocr.translated ?? ocr.text}</div>
              <div className="row gap">
                <button type="button" className="chip" onClick={() => navigator.clipboard.writeText(ocr.translated ?? ocr.text).catch(() => {})}>
                  <Copy size={12} /> Копировать
                </button>
                {!ocr.translated && (
                  <button
                    type="button"
                    className="chip"
                    onClick={() =>
                      ipc
                        .translate(ocr.text)
                        .then(([t]) => setOcr({ ...ocr, translated: t }))
                        .catch((e) => setError(errorMessage(e)))
                    }
                  >
                    <Languages size={12} /> Перевести
                  </button>
                )}
                <span className="grow" />
                <button type="button" className="chip" onClick={() => setOcr(null)}>
                  <X size={12} />
                </button>
              </div>
            </>
          )}
        </div>
      )}
      {error && <Note kind="error">{error}</Note>}
    </div>
  );
}
