// Заметки: быстрая запись прямо в острове, редактирование по клику, цвета, закрепление.

import { useEffect, useRef, useState } from "react";
import { Bell, Mic, Pin, PinOff, Play, Plus, Square, Trash2, X } from "lucide-react";
import { IconButton, Note as Hint } from "../components/ui";
import { useEvent, useLoad, useNow } from "../hooks/common";
import { errorMessage, ipc } from "../services/ipc";
import type { Note } from "../types";
import { formatRelative } from "../utils/format";
import { parseReminder, reminderLabel } from "../utils/reminder";
import { playVoice } from "../utils/voice";

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
      {(n.remindAt || n.audio) && (
        <div className="note-tags">
          {n.remindAt && (
            <span className={`note-remind ${n.reminded ? "done" : ""}`}>
              <Bell size={12} />
              {reminderLabel(n.remindAt, new Date(now))}
            </span>
          )}
          {n.audio && (
            <button type="button" className="note-play" onClick={() => playVoice(n.audio!).catch((e) => onError(errorMessage(e)))}>
              <Play size={12} />
              {Math.max(1, Math.round(n.audioMs / 1000))} с
            </button>
          )}
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

  const [recording, setRecording] = useState<{ since: number } | null>(null);
  const [levels, setLevels] = useState<number[]>([]);
  useEvent<number>("voice://level", (l) => setLevels((v) => [...v.slice(-23), l]));
  const parsed = parseReminder(draft);

  const add = async () => {
    if (!draft.trim()) return;
    try {
      const r = parseReminder(draft);
      await ipc.noteCreate(r && r.text ? r.text : draft, undefined, { remindAt: r?.at ?? null });
      setDraft("");
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const toggleRecord = async () => {
    setError(null);
    try {
      if (!recording) {
        await ipc.voiceStart();
        setLevels([]);
        setRecording({ since: Date.now() });
        return;
      }
      setRecording(null);
      const rec = await ipc.voiceStop();
      const r = parseReminder(draft);
      await ipc.noteCreate(r?.text || draft, undefined, { remindAt: r?.at ?? null, audio: rec.name, audioMs: rec.durationMs });
      setDraft("");
    } catch (e) {
      setRecording(null);
      setError(errorMessage(e));
    }
  };

  const list = notes.data ?? [];
  return (
    <div className="card-pad">
      <div className="note-new">
        <textarea
          className="note-input"
          placeholder="Заметка или «позвонить маме в 18:30»"
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
        <div className="note-buttons">
          <IconButton title={recording ? "Остановить и сохранить" : "Голосовая заметка"} active={!!recording} onClick={toggleRecord}>
            {recording ? <Square size={14} /> : <Mic size={16} />}
          </IconButton>
          {recording ? (
            <IconButton
              title="Отменить запись"
              onClick={() => {
                setRecording(null);
                ipc.voiceCancel().catch(() => {});
              }}
            >
              <X size={16} />
            </IconButton>
          ) : (
            <IconButton title="Добавить" onClick={add} disabled={!draft.trim()}>
              <Plus size={16} />
            </IconButton>
          )}
        </div>
      </div>
      {recording && (
        <div className="voice-rec" aria-live="polite">
          <span className="voice-dot" />
          <span className="voice-wave" aria-hidden>
            {Array.from({ length: 24 }, (_, i) => (
              <i key={i} style={{ transform: `scaleY(${Math.max(0.12, Math.min(1, (levels[i] ?? 0) * 3))})` }} />
            ))}
          </span>
          <span className="small muted mono">{Math.floor((now - recording.since) / 1000) >= 0 ? "идёт запись" : ""}</span>
        </div>
      )}
      {!recording && parsed && (
        <div className="note-hint small">
          <Bell size={12} />
          Напомню {reminderLabel(parsed.at)}
        </div>
      )}
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
