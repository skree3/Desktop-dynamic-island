import { useState } from "react";
// Отображение текущего уведомления из очереди Event Manager.

import { Bell, ExternalLink, Hourglass, Puzzle, X } from "lucide-react";
import { ipc } from "../services/ipc";
import { useIsland } from "../stores/island";
import type { IslandNotification } from "../types";
import { formatTime } from "../utils/format";
import { IconButton } from "./ui";

function KindIcon({ n }: { n: IslandNotification }) {
  if (n.icon && n.icon.startsWith("data:image/")) return <img className="notif-icon" src={n.icon} alt="" />;
  const Icon = n.kind === "timer" ? Hourglass : n.kind === "plugin" ? Puzzle : Bell;
  return (
    <span className={`notif-icon placeholder prio-${n.priority}`}>
      <Icon size={18} />
    </span>
  );
}

export function NotificationView({ n }: { n: IslandNotification }) {
  const queued = useIsland((s) => s.notification.queued);
  const [reply, setReply] = useState<string | null>(null);
  const send = () => {
    if (reply?.trim()) ipc.notificationAction(n.id, `reply:${reply.trim()}`).catch(() => {});
  };
  const canOpen = n.kind === "system";
  return (
    <div className="notif" onMouseEnter={() => ipc.notificationHold(n.id, true).catch(() => {})} onMouseLeave={() => ipc.notificationHold(n.id, false).catch(() => {})}>
      <span className="notif-fly" key={n.id}>
        <KindIcon n={n} />
      </span>
      <div className="notif-body" key={`b-${n.id}`}>
        <div className="notif-head">
          <span className="notif-app">{n.appName}</span>
          {(n.count ?? 1) > 1 && <span className="notif-count">{n.count}</span>}
          <span className="notif-time">{formatTime(n.timestamp)}</span>
          {queued > 0 && <span className="badge">+{queued}</span>}
        </div>
        <div className="notif-title" key={`t-${n.timestamp}`}>{n.title}</div>
        {n.body && <div className="notif-text">{n.body}</div>}
        {!!n.stack?.length && (
          <div className="notif-stack">
            {n.stack.slice(0, 2).map((m) => (
              <div key={m.timestamp} className="notif-stack-line">
                {m.title}
                {m.body ? `: ${m.body}` : ""}
              </div>
            ))}
          </div>
        )}
        {reply !== null ? (
          <div className="notif-actions notif-reply" data-no-drag>
            <input
              className="reply-input"
              autoFocus
              value={reply}
              maxLength={2000}
              placeholder="Ответ"
              aria-label="Ответ"
              onChange={(e) => setReply(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") send();
                if (e.key === "Escape") {
                  e.stopPropagation();
                  setReply(null);
                }
              }}
            />
            <button type="button" className="chip" onClick={send} disabled={!reply.trim()}>
              Отправить
            </button>
          </div>
        ) : n.actions.length > 0 && (
          <div className="notif-actions">
            {n.actions.map((a) => (
              <button
                key={a.id}
                type="button"
                className="chip"
                onClick={() => (a.id === "reply" ? (setReply(""), ipc.notificationHold(n.id, true).catch(() => {})) : ipc.notificationAction(n.id, a.id).catch(() => {}))}
              >
                {a.label}
              </button>
            ))}
          </div>
        )}
      </div>
      <div className="notif-side">
        <IconButton size="sm" title="Закрыть" onClick={() => ipc.notificationDismiss(n.id)}>
          <X size={14} />
        </IconButton>
        {canOpen && (
          <IconButton size="sm" title="Открыть приложение" onClick={() => ipc.notificationAction(n.id, "open").catch(() => {})}>
            <ExternalLink size={13} />
          </IconButton>
        )}
      </div>
    </div>
  );
}
