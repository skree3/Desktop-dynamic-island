// Полка: перетащите файлы на остров — они появятся здесь. Хранятся только ссылки.

import { useState } from "react";
import { Copy, ExternalLink, File, Folder, FolderOpen, Inbox, Trash2, X } from "lucide-react";
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

  const run = (fn: () => Promise<unknown>) => {
    setError(null);
    fn().catch((e) => setError(errorMessage(e)));
  };
  const list = items.data ?? [];
  return (
    <div className="card-pad">
      {(dragging || (items.data && list.length === 0)) && (
        <div className={`shelf-drop ${dragging ? "over" : ""}`}>
          <Inbox size={26} />
          <span>{dragging ? "Отпустите, чтобы положить на полку" : "Перетащите сюда файлы или папки"}</span>
        </div>
      )}
      {list.length > 0 && (
        <>
          <div className="row between section-title" style={{ marginTop: 4 }}>
            <span>Файлы ({list.length})</span>
            <div className="row" style={{ gap: 2 }}>
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
              <div key={it.id} className={`list-item ${it.exists ? "" : "missing"}`} onDoubleClick={() => it.exists && run(() => ipc.shelfCommand("open", it.id))} title={it.path}>
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
      {error && <Note kind="error">{error}</Note>}
    </div>
  );
}
