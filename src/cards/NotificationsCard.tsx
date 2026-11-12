import { Bell, BellOff, Trash2, X } from "lucide-react";
import { IconButton, Note, Toggle } from "../components/ui";
import { useEvent, useLoad, useNow } from "../hooks/common";
import { events, ipc } from "../services/ipc";
import { useApp } from "../stores/app";
import type { IslandNotification, ListenerStatus } from "../types";
import { formatRelative } from "../utils/format";

export default function NotificationsCard() {
  const history = useLoad(() => ipc.notificationsHistory());
  const status = useLoad(() => ipc.listenerStatus());
  const dnd = useApp((s) => s.settings?.notifications.dnd ?? false);
  const now = useNow(30_000);
  useEvent(events.NOTIFICATION, () => history.reload());
  useEvent<ListenerStatus>(events.NOTIFICATIONS_STATUS, (s) => status.setData(s));

  const list = history.data ?? [];
  return (
    <div className="card-pad">
      <div className="row between">
        <div className="row gap">
          {dnd ? <BellOff size={16} /> : <Bell size={16} />}
          <span>Не беспокоить</span>
        </div>
        <Toggle checked={dnd} label="Не беспокоить" onChange={(v) => useApp.getState().update("notifications", { dnd: v })} />
      </div>
      {status.data && !status.data.available && (
        <Note kind="warn">
          {status.data.message}
          <div className="small muted">Внутренние уведомления (таймеры, плагины) работают в любом случае.</div>
        </Note>
      )}
      <div className="row between section-title">
        <span>История ({list.length})</span>
        {list.length > 0 && (
          <IconButton size="sm" title="Очистить историю" onClick={() => ipc.notificationsClearHistory().then(history.reload)}>
            <Trash2 size={14} />
          </IconButton>
        )}
      </div>
      {list.length === 0 ? (
        <div className="empty small">Уведомлений пока нет</div>
      ) : (
        <div className="list scroll" style={{ maxHeight: 300 }}>
          {list.map((n: IslandNotification) => (
            <div key={n.id} className={`list-item prio-${n.priority}`}>
              <div className="grow">
                <div className="row between">
                  <span className="small strong">{n.appName}</span>
                  <span className="small muted">{formatRelative(n.timestamp, now)}</span>
                </div>
                <div className="ellipsis">{n.title}</div>
                {n.body && <div className="small muted clamp2">{n.body}</div>}
                {n.actions.length > 0 && (
                  <div className="notif-actions">
                    {n.actions.map((a) => (
                      <button key={a.id} type="button" className="chip" onClick={() => ipc.notificationAction(n.id, a.id).then(history.reload).catch(() => {})}>
                        {a.label}
                      </button>
                    ))}
                  </div>
                )}
              </div>
              <IconButton size="sm" title="Удалить" onClick={() => ipc.notificationRemove(n.id).then(history.reload)}>
                <X size={13} />
              </IconButton>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
