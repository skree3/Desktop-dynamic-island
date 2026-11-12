// Статус интеграций: установлено ли приложение, запущено ли, уровень поддержки.

import { useState } from "react";
import { Circle, ExternalLink, Play, RefreshCw, Shield } from "lucide-react";
import { AppIcon, IconButton, Note, Spinner } from "../components/ui";
import { useLoad } from "../hooks/common";
import { errorMessage, ipc } from "../services/ipc";
import type { IntegrationStatus } from "../types";

const CATEGORY: Record<string, string> = { music: "Музыка", browser: "Браузеры", chat: "Общение", games: "Игры", proxy: "Прокси", ai: "Нейросети" };

export default function IntegrationsCard() {
  const st = useLoad(() => ipc.integrations());
  const [error, setError] = useState<string | null>(null);
  const [open, setOpen] = useState<string | null>(null);

  if (!st.data) return <div className="card-pad center">{st.error ? <Note kind="error">{st.error}</Note> : <Spinner />}</div>;
  const groups = Object.keys(CATEGORY).map((c) => [c, st.data!.filter((i) => i.category === c)] as const);
  return (
    <div className="card-pad">
      <div className="row between">
        <span className="small muted">Фактический уровень поддержки на этой системе</span>
        <IconButton size="sm" title="Обновить" onClick={() => st.reload()}>
          <RefreshCw size={13} />
        </IconButton>
      </div>
      {error && <Note kind="error">{error}</Note>}
      <div className="list scroll" style={{ maxHeight: 360 }}>
        {groups.map(([cat, items]) =>
          items.length ? (
            <div key={cat}>
              <div className="section-title">{CATEGORY[cat]}</div>
              {items.map((i: IntegrationStatus) => (
                <div key={i.id} className="integration">
                  <div className="list-item" onClick={() => setOpen(open === i.id ? null : i.id)}>
                    {i.appId ? <AppIcon id={i.appId} size={22} /> : <Circle size={10} className="muted" />}
                    <div className="grow">
                      <div className="row gap">
                        <span>{i.name}</span>
                        {i.running && <span className="dot on" title="Запущено" />}
                      </div>
                      <div className={`small level-${i.support}`}>{i.supportLabel}</div>
                    </div>
                    {i.appId && (
                      <IconButton size="sm" title="Запустить" onClick={() => ipc.appsLaunch(i.appId!).catch((e) => setError(errorMessage(e)))}>
                        <Play size={13} />
                      </IconButton>
                    )}
                  </div>
                  {open === i.id && (
                    <div className="integration-details small">
                      <div>{i.notes}</div>
                      <div className="muted">{i.installed ? "Установлено" : "Не найдено среди установленных программ"}{i.running ? " · запущено" : ""}</div>
                      {i.proxy && (
                        <div className="row gap">
                          <Shield size={13} />
                          Системный прокси: {i.proxy.enabled ? `включён${i.proxy.server ? ` (${i.proxy.server})` : ""}` : "выключен"}
                          {i.proxy.autoConfig ? ` · PAC: ${i.proxy.autoConfig}` : ""}
                        </div>
                      )}
                      {i.webUrl && (
                        <button type="button" className="link row gap" onClick={() => ipc.openUrl(i.webUrl!).catch((e) => setError(errorMessage(e)))}>
                          <ExternalLink size={12} /> {i.webUrl}
                        </button>
                      )}
                    </div>
                  )}
                </div>
              ))}
            </div>
          ) : null,
        )}
      </div>
    </div>
  );
}
