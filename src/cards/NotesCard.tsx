// Заметки: быстрая запись прямо в острове, редактирование по клику, цвета, закрепление.

import { useEffect, useRef, useState } from "react";
import { Pin, PinOff, Plus, Trash2 } from "lucide-react";
import { IconButton, Note as Hint } from "../components/ui";
import { useEvent, useLoad, useNow } from "../hooks/common";
import { errorMessage, ipc } from "../services/ipc";
import type { Note } from "../types";
import { formatRelative } from "../utils/format";

const COLORS: Record<string, string> = {
  default: "rgba(255,255,255,0.07)",
  yellow: "rgba(255, 204, 0, 0.18)",
  green: "rgba(60, 207, 145, 0.18)",
  blue: "rgba(76, 154, 255, 0.2)",
  pink: "rgba(255, 95, 162, 0.18)",
  purple: "rgba(166, 107, 255, 0.2)",
};

function NoteItem({ n, now, onError }: { n: Note; now: number; onError: (e: string) => void }) {
  const [editing, setEditing] = useState(false);
  const [text, setText] = useState(n.text);
  const ref = useRef<HTMLTextAreaElement>(null);
  useEffect(() => setText(n.text), [n.text]);
  useEffect(() => {
    if (editing) ref.current?.focus();
  }, [editing]);

  const save = () => {
    setEditing(false);
    if (text.trim() && text !== n.text) ipc.noteUpdate(n.id, { text }).catch((e) => onError(errorMessage(e)));
    else setText(n.text);
  };
  const colors = Object.keys(COLORS);
  return (
    <div className="note-item" style={{ background: COLORS[n.color] ?? COLORS.default }}>
      {editing ? (
        <textarea
          ref={ref}
          className="note-edit"
          value={text}
          maxLength={10000}
          onChange={(e) => setText(e.target.value)}
          onBlur={save}
          onKeyDown={(e) => {
            if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) save();
            if (e.key === "Escape") {
              e.stopPropagation();
              setText(n.text);
              setEditing(false);
            }
          }}
        />
      ) : (
        <div className="note-text" onClick={() => setEditing(true)} title="Нажмите, чтобы изменить">
          {n.text}
        </div>
      )}
      <div className="note-meta">
        <span className="small muted">{formatRelative(n.updatedAt, now)}</span>
        <div className="row" style={{ gap: 2 }}>
          <button
            type="button"
            className="note-color"
            title="Цвет"
            style={{ background: COLORS[colors[(colors.indexOf(n.color) + 1) % colors.length]] }}
            onClick={() => ipc.noteUpdate(n.id, { color: colors[(colors.indexOf(n.color) + 1) % colors.length] })}
          />
          <IconButton size="sm" title={n.pinned ? "Открепить" : "Закрепить"} active={n.pinned} onClick={() => ipc.noteUpdate(n.id, { pinned: !n.pinned })}>
            {n.pinned ? <PinOff size={13} /> : <Pin size={13} />}
          </IconButton>
          <IconButton size="sm" title="Удалить" onClick={() => ipc.noteDelete(n.id)}>
            <Trash2 size={13} />
          </IconButton>
        </div>
      </div>
    </div>
  );
}

export default function NotesCard() {
  const notes = useLoad(() => ipc.notes());
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);
  const now = useNow(60_000);
  useEvent("notes://changed", () => notes.reload());

  const add = async () => {
    if (!draft.trim()) return;
    try {
      await ipc.noteCreate(draft);
      setDraft("");
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const list = notes.data ?? [];
  return (
    <div className="card-pad">
      <div className="note-new">
        <textarea
          className="note-input"
          placeholder="Новая заметка… (Enter — сохранить, Shift+Enter — новая строка)"
          value={draft}
          rows={2}
          maxLength={10000}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              add();
            }
          }}
        />
        <IconButton title="Добавить" onClick={add} disabled={!draft.trim()}>
          <Plus size={16} />
        </IconButton>
      </div>
      {error && <Hint kind="error">{error}</Hint>}
      <div className="notes-list scroll" style={{ maxHeight: 300 }}>
        {notes.data && list.length === 0 && <div className="empty small">Заметок пока нет</div>}
        {list.map((n) => (
          <NoteItem key={n.id} n={n} now={now} onError={setError} />
        ))}
      </div>
    </div>
  );
}
