// Раздел «Интеграции»: уровни поддержки известных приложений, ручное добавление
// программ и веб-сервисов, выбор способа интеграции (только реально доступные).

import { useEffect, useState } from "react";
import { Check, FolderOpen, Globe, Pencil, Plus, Search, X } from "lucide-react";
import { AppIcon, Button, IconButton, Note, Segmented, Spinner, Toggle } from "../../components/ui";
import { useAction, useLoad } from "../../hooks/common";
import { ipc } from "../../services/ipc";
import type { AppEntry, Capability, CustomApp, IntegrationStatus } from "../../types";
import { ConfirmButton, Field, Group, Page } from "../components";

const CATEGORY: Record<string, string> = { music: "Музыка", browser: "Браузеры", chat: "Общение", games: "Игры и платформы", proxy: "Прокси-приложения", ai: "Нейросети" };

export function IntegrationsSection() {
  const status = useLoad(() => ipc.integrations());
  const custom = useLoad(() => ipc.customApps());
  const [editing, setEditing] = useState<CustomApp | "new" | null>(null);
  const { error, run } = useAction();

  return (
    <Page
      title="Интеграции"
      description="Список не закрыт: любые установленные программы находятся поиском, а новые можно добавить вручную."
      actions={<Button variant="primary" onClick={() => setEditing("new")}><Plus size={14} /> Добавить приложение</Button>}
    >
      {editing && (
        <AppEditor
          initial={editing === "new" ? null : editing}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null);
            custom.reload();
            status.reload();
          }}
        />
      )}

      <Group title="Добавленные вручную">
        {custom.data?.length === 0 && <div className="small muted">Пока ничего не добавлено.</div>}
        {custom.data?.map((c) => (
          <Field
            key={c.id}
            label={
              <span className="row gap">
                <AppIcon id={c.id} size={20} fallback={c.kind === "web" ? <Globe size={13} /> : undefined} />
                {c.name}
              </span>
            }
            description={`${c.kind === "web" ? "Веб-сервис" : "Программа"} · ${c.target} · интеграция: ${c.adapter}${c.notificationsEnabled ? "" : " · уведомления отключены"}`}
          >
            <div className="row gap">
              <IconButton size="sm" title="Изменить" onClick={() => setEditing(c)}>
                <Pencil size={14} />
              </IconButton>
              <ConfirmButton small label="Удалить" title={`Удалить «${c.name}»?`} text="Запись будет удалена из Desktop Island. Сама программа не удаляется." onConfirm={() => run(async () => { await ipc.customAppDelete(c.id); custom.reload(); })} />
            </div>
          </Field>
        ))}
      </Group>

      <Group title="Известные приложения и фактический уровень поддержки">
        {!status.data && <Spinner />}
        {status.error && <Note kind="error">{status.error}</Note>}
        {status.data &&
          Object.keys(CATEGORY).map((cat) => {
            const items = status.data!.filter((i) => i.category === cat);
            if (!items.length) return null;
            return (
              <div key={cat}>
                <div className="section-title">{CATEGORY[cat]}</div>
                {items.map((i: IntegrationStatus) => (
                  <Field
                    key={i.id}
                    label={
                      <span className="row gap">
                        {i.name}
                        {i.running && <span className="dot on" title="Запущено" />}
                        <span className={`pill level-${i.support}`}>{i.supportLabel}</span>
                      </span>
                    }
                    description={
                      <>
                        {i.notes}
                        <br />
                        <span className="muted">{i.installed ? "Установлено" : "Не найдено среди установленных"}</span>
                        {i.proxy && <span className="muted"> · системный прокси {i.proxy.enabled ? `включён${i.proxy.server ? ` (${i.proxy.server})` : ""}` : "выключен"} — {i.proxy.source}</span>}
                      </>
                    }
                  >
                    {i.appId && <Button small onClick={() => run(() => ipc.appsLaunch(i.appId!))}>Запустить</Button>}
                  </Field>
                ))}
              </div>
            );
          })}
      </Group>
      {error && <Note kind="error">{error}</Note>}
    </Page>
  );
}

type Mode = "installed" | "exe" | "web";

function targetFor(a: AppEntry): string {
  switch (a.target.type) {
    case "appsFolder":
      return `shell:AppsFolder\\${a.target.id}`;
    case "exe":
      return a.target.path;
    case "desktop":
      return a.target.path;
    case "uri":
      return a.target.uri;
  }
}

function AppEditor({ initial, onClose, onSaved }: { initial: CustomApp | null; onClose: () => void; onSaved: () => void }) {
  const [mode, setMode] = useState<Mode>(initial ? (initial.kind === "web" ? "web" : "exe") : "installed");
  const [name, setName] = useState(initial?.name ?? "");
  const [target, setTarget] = useState(initial?.target ?? "");
  const [args, setArgs] = useState(initial?.args ?? "");
  const [icon, setIcon] = useState(initial?.icon ?? "");
  const [catalogId, setCatalogId] = useState<string | null>(initial?.catalogId ?? null);
  const [adapter, setAdapter] = useState(initial?.adapter ?? "launch");
  const [notifications, setNotifications] = useState(initial?.notificationsEnabled ?? true);
  const [showCard, setShowCard] = useState(initial?.showCard ?? true);
  const [favorite, setFavorite] = useState(initial?.favorite ?? false);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<AppEntry[]>([]);
  const [caps, setCaps] = useState<Capability[]>([]);
  const { error, busy, run } = useAction();
  const kind = mode === "web" ? "web" : "custom";

  useEffect(() => {
    if (mode !== "installed") return;
    const t = setTimeout(() => ipc.appsSearch(query, 12).then((r) => setResults(r.filter((x) => x.source === "system"))).catch(() => {}), 120);
    return () => clearTimeout(t);
  }, [query, mode]);

  useEffect(() => {
    if (!target) return setCaps([]);
    ipc.capabilities(kind, target, catalogId).then((c) => {
      setCaps(c);
      if (!c.some((x) => x.id === adapter && x.available)) setAdapter(c.find((x) => x.available)?.id ?? "launch");
    }).catch(() => setCaps([]));
  }, [kind, target, catalogId]);

  const save = () =>
    run(async () => {
      await ipc.customAppSave({
        id: initial?.id ?? null,
        name,
        kind,
        target,
        args,
        icon,
        adapter,
        catalogId,
        notificationsEnabled: notifications,
        showCard,
        favorite,
      });
      onSaved();
    });

  return (
    <div className="editor">
      <div className="row between">
        <h2>{initial ? "Изменить приложение" : "Добавить приложение"}</h2>
        <IconButton title="Закрыть" onClick={onClose}>
          <X size={16} />
        </IconButton>
      </div>
      {!initial && (
        <Segmented<Mode>
          value={mode}
          onChange={(m) => {
            setMode(m);
            setTarget("");
            setCatalogId(null);
          }}
          options={[
            { value: "installed", label: "Установленная программа" },
            { value: "exe", label: "Путь к файлу" },
            { value: "web", label: "Веб-сервис" },
          ]}
        />
      )}

      {mode === "installed" && !initial && (
        <div>
          <div className="search" style={{ marginTop: 10 }}>
            <Search size={14} />
            <input className="input-bare" placeholder="Найти среди установленных" value={query} onChange={(e) => setQuery(e.target.value)} />
          </div>
          <div className="pick-list">
            {results.map((r) => (
              <button
                key={r.id}
                type="button"
                className={`pick ${targetFor(r) === target ? "on" : ""}`}
                onClick={() => {
                  setTarget(targetFor(r));
                  setName(r.name);
                  setCatalogId(r.catalogId);
                }}
              >
                <AppIcon id={r.id} size={20} />
                <span className="ellipsis">{r.name}</span>
                {targetFor(r) === target && <Check size={14} />}
              </button>
            ))}
          </div>
        </div>
      )}

      <div className="form">
        {mode === "exe" && (
          <Field label="Исполняемый файл" wide>
            <div className="row gap">
              <input className="input grow" value={target} placeholder={navigator.userAgent.includes("Windows") ? "C:\\Program Files\\App\\app.exe" : "/usr/bin/app"} onChange={(e) => setTarget(e.target.value)} />
              <Button onClick={() => run(async () => { const p = await ipc.pickPath("exe"); if (p) { setTarget(p); if (!name) setName(p.split(/[\\/]/).pop()!.replace(/\.(exe|lnk)$/i, "")); } })}>
                <FolderOpen size={14} /> Обзор
              </Button>
            </div>
          </Field>
        )}
        {mode === "web" && (
          <Field label="Адрес (https)" wide>
            <input className="input" style={{ width: "100%" }} value={target} placeholder="https://example.com" onChange={(e) => setTarget(e.target.value)} />
          </Field>
        )}
        <Field label="Название" wide>
          <input className="input" style={{ width: "100%" }} value={name} maxLength={80} onChange={(e) => setName(e.target.value)} />
        </Field>
        {mode !== "web" && (
          <Field label="Аргументы запуска" description="Необязательно" wide>
            <input className="input" style={{ width: "100%" }} value={args} onChange={(e) => setArgs(e.target.value)} />
          </Field>
        )}
        <Field label="Иконка" description="PNG, JPG, SVG, ICO до 512 КБ. По умолчанию — иконка программы.">
          <div className="row gap">
            {icon && icon.startsWith("data:") && <img src={icon} width={24} height={24} alt="" />}
            {icon && !icon.startsWith("data:") && <span className="small muted ellipsis" style={{ maxWidth: 200 }}>{icon}</span>}
            <Button small onClick={() => run(async () => { const p = await ipc.pickPath("icon"); if (p) setIcon(p); })}>Выбрать</Button>
            {icon && <Button small onClick={() => setIcon("")}>Сбросить</Button>}
          </div>
        </Field>
      </div>

      {caps.length > 0 && (
        <div>
          <div className="section-title">Способ интеграции</div>
          <div className="caps">
            {caps.map((c) => (
              <label key={c.id} className={`cap ${c.available ? "" : "disabled"}`} title={c.reason}>
                <input type="radio" name="adapter" disabled={!c.available} checked={adapter === c.id} onChange={() => setAdapter(c.id)} />
                <span>
                  <strong>{c.label}</strong>
                  <small>{c.available ? c.reason : `Недоступно: ${c.reason}`}</small>
                </span>
              </label>
            ))}
          </div>
        </div>
      )}

      <div className="form">
        <Field label="Уведомления этого приложения" description={caps.find((c) => c.id === "notifications")?.available === false ? "Получение уведомлений ОС сейчас недоступно, настройка применится, когда станет доступно." : undefined}>
          <Toggle checked={notifications} onChange={setNotifications} />
        </Field>
        <Field label="Показывать в поиске и карточках">
          <Toggle checked={showCard} onChange={setShowCard} />
        </Field>
        <Field label="В избранное">
          <Toggle checked={favorite} onChange={setFavorite} />
        </Field>
      </div>
      {error && <Note kind="error">{error}</Note>}
      <div className="row gap" style={{ justifyContent: "flex-end" }}>
        <Button onClick={onClose}>Отмена</Button>
        <Button variant="primary" disabled={busy || !name.trim() || !target.trim()} onClick={save}>
          {busy ? <Spinner size={14} /> : "Сохранить"}
        </Button>
      </div>
    </div>
  );
}

