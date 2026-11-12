// Раздел «Плагины»: установка, обновление, включение с подтверждением прав,
// настройки плагина, удаление и журнал ошибок.

import { useState } from "react";
import { FolderOpen, Puzzle, RefreshCw } from "lucide-react";
import { Button, Note, Spinner, Toggle } from "../../components/ui";
import { useAction, useEvent, useLoad } from "../../hooks/common";
import { events, ipc } from "../../services/ipc";
import { useApp } from "../../stores/app";
import type { PluginInfo } from "../../types";
import { formatDateTime } from "../../utils/format";
import { ConfirmButton, Field, Group, Page } from "../components";

export function PluginsSection() {
  const list = useLoad(() => ipc.plugins());
  const perms = useLoad(() => ipc.pluginPermissions());
  const security = useApp((s) => s.settings!.security);
  const modules = useApp((s) => s.settings!.modules);
  const [approve, setApprove] = useState<PluginInfo | null>(null);
  const [info, setInfo] = useState<string | null>(null);
  const { error, run, busy } = useAction();
  useEvent(events.PLUGINS_CHANGED, () => list.reload());
  const permLabel = (p: string) => perms.data?.find(([k]) => k === p)?.[1] ?? p;

  const install = () =>
    run(async () => {
      const path = await ipc.pickPath("plugin");
      if (!path) return;
      const r = await ipc.pluginInstall(path);
      setInfo(r.updated ? `Плагин «${r.plugin.manifest.name}» обновлён: ${r.previousVersion} → ${r.plugin.manifest.version}` : `Плагин «${r.plugin.manifest.name}» установлен. Включите его, чтобы выдать разрешения.`);
      list.reload();
    });

  const enabledGlobally = modules.plugins && security.pluginsEnabled;
  return (
    <Page
      title="Плагины"
      description="Плагины выполняются в изолированной песочнице без доступа к системе, сети и файлам. Возможности — только через разрешения, которые вы подтверждаете."
      actions={
        <>
          <Button onClick={() => run(() => ipc.openPath("plugins"))}>
            <FolderOpen size={14} /> Папка плагинов
          </Button>
          <Button variant="primary" onClick={install} disabled={busy}>
            {busy ? <Spinner size={14} /> : "Установить из папки…"}
          </Button>
        </>
      }
    >
      {!enabledGlobally && <Note kind="warn">Плагины отключены (раздел «Карточки» или «Безопасность»). Установка возможна, но плагины не запускаются.</Note>}
      {info && <Note kind="info">{info}</Note>}
      {error && <Note kind="error">{error}</Note>}

      {approve && (
        <div className="editor">
          <h2>Разрешения для «{approve.manifest.name}»</h2>
          <p className="muted">Плагин запрашивает:</p>
          <ul>
            {approve.manifest.permissions.length === 0 && <li>Ничего — только отображение собственной карточки</li>}
            {approve.manifest.permissions.map((p) => (
              <li key={p}>
                <strong>{p}</strong> — {permLabel(p)}
              </li>
            ))}
          </ul>
          {approve.manifest.allowedHosts.length > 0 && <p className="small muted">Разрешённые сайты: {approve.manifest.allowedHosts.join(", ")}</p>}
          <div className="row gap" style={{ justifyContent: "flex-end" }}>
            <Button onClick={() => setApprove(null)}>Отмена</Button>
            <Button
              variant="primary"
              onClick={() =>
                run(async () => {
                  await ipc.pluginSetEnabled(approve.manifest.id, true, true);
                  setApprove(null);
                  list.reload();
                })
              }
            >
              Разрешить и включить
            </Button>
          </div>
        </div>
      )}

      <Group title="Установленные">
        {list.data?.length === 0 && (
          <div className="empty">
            <Puzzle size={26} />
            <span>Плагины не установлены. Пример — в каталоге examples/plugins репозитория, документация — docs/PLUGIN_API.md.</span>
          </div>
        )}
        {list.data?.map((p) => <PluginRow key={p.manifest.id} p={p} onApprove={() => setApprove(p)} reload={list.reload} run={run} permLabel={permLabel} />)}
      </Group>
      <LogsFor />
    </Page>
  );
}

function PluginRow({ p, onApprove, reload, run, permLabel }: { p: PluginInfo; onApprove: () => void; reload: () => void; run: ReturnType<typeof useAction>["run"]; permLabel: (s: string) => string }) {
  const [open, setOpen] = useState(false);
  const m = p.manifest;
  return (
    <div className="plugin-row">
      <Field
        label={
          <span className="row gap">
            {m.name} <span className="pill">{m.version}</span> {p.builtin && <span className="pill level-full">встроенный</span>} {p.missing && <span className="pill level-launch_only">файлы отсутствуют</span>}
          </span>
        }
        description={`${m.description || m.id}${m.author ? ` · ${m.author}` : ""} · API v${m.apiVersion}`}
      >
        <div className="row gap">
          <Button small onClick={() => setOpen(!open)}>{open ? "Скрыть" : "Подробнее"}</Button>
          <Toggle
            checked={p.enabled}
            onChange={(v) =>
              v && !m.permissions.every((x) => p.granted.includes(x))
                ? onApprove()
                : run(async () => {
                    await ipc.pluginSetEnabled(m.id, v, false);
                    reload();
                  })
            }
          />
        </div>
      </Field>
      {open && (
        <div className="plugin-details">
          <div className="small">
            Разрешения: {m.permissions.length ? m.permissions.map((x) => `${x} (${permLabel(x)})`).join(", ") : "нет"}
            {p.pendingPermissions.length > 0 && <div className="accent">Ожидают подтверждения: {p.pendingPermissions.join(", ")}</div>}
          </div>
          <div className="small muted">Установлен: {formatDateTime(p.installedAt)} · обновлён: {formatDateTime(p.updatedAt)} · {p.path}</div>
          {m.settings.length > 0 && <PluginSettings p={p} />}
          <div className="row gap" style={{ marginTop: 8 }}>
            <Button small onClick={() => run(async () => { const path = await ipc.pickPath("plugin"); if (path) { await ipc.pluginInstall(path); reload(); } })}>
              <RefreshCw size={12} /> Обновить из папки…
            </Button>
            {!p.builtin && <ConfirmButton small label="Удалить" title={`Удалить плагин «${m.name}»?`} text="Файлы и настройки плагина будут удалены." onConfirm={() => run(async () => { await ipc.pluginUninstall(m.id); reload(); })} />}
          </div>
        </div>
      )}
    </div>
  );
}

function PluginSettings({ p }: { p: PluginInfo }) {
  const values = useLoad(() => ipc.pluginSettings(p.manifest.id), [p.manifest.id]);
  const [error, setError] = useState<string | null>(null);
  const set = (key: string, value: unknown) =>
    ipc.pluginSettingSet(p.manifest.id, key, value).then(values.reload).catch((e) => setError(String(e?.message ?? e)));
  if (!values.data) return <Spinner />;
  return (
    <div className="form">
      {p.manifest.settings.map((s) => {
        const v = values.data![s.key];
        return (
          <Field key={s.key} label={s.label}>
            {s.type === "boolean" ? (
              <Toggle checked={!!v} onChange={(x) => set(s.key, x)} />
            ) : s.type === "select" ? (
              <select className="input" value={String(v ?? "")} onChange={(e) => set(s.key, e.target.value)}>
                {s.options.map((o) => (
                  <option key={o}>{o}</option>
                ))}
              </select>
            ) : (
              <input className="input" type={s.type === "number" ? "number" : "text"} defaultValue={String(v ?? "")} onBlur={(e) => set(s.key, s.type === "number" ? Number(e.target.value) : e.target.value)} />
            )}
          </Field>
        );
      })}
      {error && <Note kind="error">{error}</Note>}
    </div>
  );
}

function LogsFor() {
  const logs = useLoad(() => ipc.logs(50, "plugin", null));
  return (
    <Group title="Журнал плагинов">
      {logs.data?.length === 0 && <div className="small muted">Записей нет</div>}
      <div className="log-list">
        {logs.data?.map((l) => (
          <div key={l.id} className={`log-row lvl-${l.level}`}>
            <span className="mono small muted">{formatDateTime(l.ts)}</span>
            <span className="small">{l.source}</span>
            <span className="grow">{l.message}</span>
          </div>
        ))}
      </div>
    </Group>
  );
}
