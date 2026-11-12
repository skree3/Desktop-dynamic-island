// Нейросети: запуск установленных приложений и веб-версии в изолированных мини-окнах.

import { useState } from "react";
import { AppWindow, Bot, ExternalLink, LogOut, Plus, X } from "lucide-react";
import { AppIcon, IconButton, Note, Spinner } from "../components/ui";
import { useLoad } from "../hooks/common";
import { errorMessage, ipc } from "../services/ipc";
import type { WebService } from "../types";

export default function AiCard() {
  const services = useLoad(() => ipc.aiServices());
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<string | null>(null);

  const run = async (key: string, fn: () => Promise<unknown>) => {
    setBusy(key);
    setError(null);
    try {
      await fn();
      await services.reload();
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(null);
    }
  };

  if (!services.data) return <div className="card-pad center">{services.error ? <Note kind="error">{services.error}</Note> : <Spinner />}</div>;
  return (
    <div className="card-pad">
      <div className="list">
        {services.data.map((s: WebService) => (
          <div key={s.id} className="list-item">
            <span className="ai-avatar">{s.appId ? <AppIcon id={s.appId} size={24} /> : <Bot size={18} />}</span>
            <div className="grow">
              <div>{s.name}</div>
              <div className="small muted ellipsis">{s.url.replace(/^https:\/\//, "")}</div>
            </div>
            {busy === s.id && <Spinner size={14} />}
            {s.appId && (
              <IconButton size="sm" title="Запустить приложение" onClick={() => run(s.id, () => ipc.appsLaunch(s.appId!))}>
                <AppWindow size={14} />
              </IconButton>
            )}
            <IconButton size="sm" title={s.open ? "Показать мини-окно" : "Открыть мини-окно"} active={s.open} onClick={() => run(s.id, () => ipc.aiOpen(s.id))}>
              <ExternalLink size={14} />
            </IconButton>
            {s.open && (
              <IconButton size="sm" title="Закрыть окно (освободить память)" onClick={() => run(s.id, () => ipc.aiClose(s.id))}>
                <X size={14} />
              </IconButton>
            )}
            <IconButton size="sm" title="Выйти: удалить cookies и данные сервиса" onClick={() => run(s.id, () => ipc.aiClearData(s.id))}>
              <LogOut size={13} />
            </IconButton>
          </div>
        ))}
      </div>
      <button type="button" className="link row gap" style={{ marginTop: 8 }} onClick={() => ipc.openSettings("integrations")}>
        <Plus size={13} /> Добавить веб-сервис
      </button>
      {error && <Note kind="error">{error}</Note>}
    </div>
  );
}
