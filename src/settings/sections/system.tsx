// Разделы: Игровой режим, Производительность, Несколько мониторов, Безопасность,
// Журнал событий, О программе.

import { useState } from "react";
import { FolderOpen, Monitor, RefreshCw } from "lucide-react";
import { Button, Note, Segmented, Spinner, Toggle } from "../../components/ui";
import { useAction, useEvent, useLoad } from "../../hooks/common";
import { events, ipc } from "../../services/ipc";
import { useApp } from "../../stores/app";
import type { GameModeState, MonitorInfo } from "../../types";
import { formatBytes, formatDateTime } from "../../utils/format";
import { ConfirmButton, Field, Group, ListEditor, NumberInput, Page } from "../components";

export function GameModeSection() {
  const g = useApp((s) => s.settings!.gameMode);
  const update = useApp((s) => s.update);
  const state = useLoad(() => ipc.gameMode());
  useEvent<GameModeState>(events.GAME_MODE, (s) => state.setData(s));
  const { error, run } = useAction();
  const set = (p: Partial<typeof g>) => run(() => update("gameMode", p));
  const manual = g.manual === null ? "auto" : g.manual ? "on" : "off";
  const reason: Record<string, string> = { manual: "включён вручную", game: "обнаружена игра", launcher: "игра запущена лаунчером", "fullscreen-game": "полноэкранная игра", exclusive: "эксклюзивный полноэкранный режим" };

  const addForeground = (field: "exclusions" | "extraGames") =>
    run(async () => {
      // Даём 3 секунды, чтобы переключиться на нужное приложение.
      await new Promise((r) => setTimeout(r, 3000));
      const fg = await ipc.foreground();
      if (fg?.exeName && !g[field].includes(fg.exeName)) set({ [field]: [...g[field], fg.exeName] } as any);
    });

  return (
    <Page title="Игровой режим" description="Панель скрывается только во время игр. Уведомления сохраняются в истории, лишние опросы и анимации приостанавливаются.">
      <Note kind={state.data?.active ? "warn" : "info"}>
        Сейчас: {state.data?.active ? `активен — ${reason[state.data.reason ?? ""] ?? state.data.reason}${state.data.app ? ` (${state.data.app})` : ""}` : "не активен"}
      </Note>
      <Group>
        <Field label="Режим работы">
          <Segmented
            value={manual}
            onChange={(v) => run(() => ipc.gameModeSetManual(v === "auto" ? null : v === "on"))}
            options={[
              { value: "auto", label: "Автоматически" },
              { value: "on", label: "Всегда вкл." },
              { value: "off", label: "Выключен" },
            ]}
          />
        </Field>
        <Field label="Автоматическое определение">
          <Toggle checked={g.auto} onChange={(v) => set({ auto: v })} />
        </Field>
        <Field label="Полноэкранные игры без лаунчера" description="Полноэкранное окно считается игрой, только если это не браузер, видеоплеер, офис, мессенджер или среда разработки. Видео на весь экран панель не скрывает.">
          <Toggle checked={g.detectFullscreen} onChange={(v) => set({ detectFullscreen: v })} />
        </Field>
        <Field label="Распознавание игр" description="Папки и лаунчеры Steam, Epic, GOG, Xbox, Riot, Lesta, Gaijin, Minecraft, Proton/Wine, эксклюзивный полноэкранный режим Direct3D.">
          <Toggle checked={g.detectGames} onChange={(v) => set({ detectGames: v })} />
        </Field>
        <Field label="Не показывать уведомления поверх игры">
          <Toggle checked={g.suppressNotifications} onChange={(v) => set({ suppressNotifications: v })} />
        </Field>
        <Field label="Интервал проверки">
          <NumberInput value={g.checkIntervalMs} min={1000} max={10000} step={500} suffix="мс" onChange={(v) => set({ checkIntervalMs: v })} />
        </Field>
      </Group>
      <Group title="Исключения (никогда не включать игровой режим)">
        <ListEditor items={g.exclusions} placeholder="program.exe" onChange={(v) => set({ exclusions: v })} extra={<Button small onClick={() => addForeground("exclusions")} title="Через 3 секунды будет добавлено активное приложение">Активное окно</Button>} />
      </Group>
      <Group title="Дополнительные игры">
        <ListEditor items={g.extraGames} placeholder="game.exe" onChange={(v) => set({ extraGames: v })} extra={<Button small onClick={() => addForeground("extraGames")} title="Через 3 секунды будет добавлено активное приложение">Активное окно</Button>} />
      </Group>
      {error && <Note kind="error">{error}</Note>}
    </Page>
  );
}

export function PerformanceSection() {
  const p = useApp((s) => s.settings!.performance);
  const update = useApp((s) => s.update);
  const mem = useLoad(() => ipc.memory());
  const set = (x: Partial<typeof p>) => update("performance", x).catch(() => {});
  return (
    <Page title="Производительность" description="Целевые показатели: простой 100–200 МБ, обычная работа 200–400 МБ, пик до 900 МБ (суммарно по всем процессам).">
      <Group title="Потребление памяти сейчас">
        {!mem.data ? (
          <Spinner />
        ) : (
          <>
            <div className="mem-total">
              <span className="mono">{formatBytes(mem.data.total)}</span>
              <Button small onClick={mem.reload}>
                <RefreshCw size={12} /> Обновить
              </Button>
            </div>
            <div className="small muted">{mem.data.note}</div>
            <table className="table">
              <tbody>
                {mem.data.processes.map((pr) => (
                  <tr key={pr.pid}>
                    <td>{pr.name}</td>
                    <td className="muted mono">{pr.pid}</td>
                    <td className="mono right">{formatBytes(pr.memory)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </>
        )}
      </Group>
      <Group title="Оптимизация">
        <Field
          label="Программная отрисовка панели (Windows)"
          description="WebView2 без GPU-композитинга: −100 МБ памяти GPU-процесса для маленького окна панели при почти нулевой нагрузке на CPU. Применяется после перезапуска приложения."
        >
          <Toggle checked={p.softwareRendering} onChange={(v) => set({ softwareRendering: v })} />
        </Field>
        <Field label="Приостанавливать опросы при скрытой панели" description="Музыка и системный монитор не опрашиваются, пока панель скрыта.">
          <Toggle checked={p.pauseWhenHidden} onChange={(v) => set({ pauseWhenHidden: v })} />
        </Field>
        <Field label="Одно мини-окно нейросети одновременно" description="Каждое мини-окно — отдельный процесс WebView (~100–250 МБ).">
          <Toggle checked={p.singleAiWindow} onChange={(v) => set({ singleAiWindow: v })} />
        </Field>
        <Field label="Закрывать мини-окна при скрытии панели">
          <Toggle checked={p.closeAiOnHide} onChange={(v) => set({ closeAiOnHide: v })} />
        </Field>
      </Group>
    </Page>
  );
}

export function MonitorsSection() {
  const monitors = useLoad(() => ipc.monitors());
  const pos = useApp((s) => s.settings!.position);
  useEvent<MonitorInfo[]>(events.MONITORS_CHANGED, (m) => monitors.setData(m));
  const { error, run } = useAction();
  return (
    <Page title="Несколько мониторов" description="Положение панели хранится отдельно для каждого монитора. При отключении монитора панель переходит на основной и возвращается при переподключении.">
      <Group title="Мониторы">
        <Field label="Монитор для панели">
          <select className="input" value={pos.monitor ?? ""} onChange={(e) => run(() => ipc.moveToMonitor(e.target.value || null))}>
            <option value="">Основной монитор</option>
            {monitors.data?.map((m) => (
              <option key={m.key} value={m.key}>
                {m.name} ({m.width}×{m.height})
              </option>
            ))}
          </select>
        </Field>
        {monitors.data?.map((m) => (
          <Field
            key={m.key}
            label={
              <span className="row gap">
                <Monitor size={14} /> {m.name} {m.primary && <span className="pill">основной</span>} {m.current && <span className="pill level-full">панель здесь</span>}
              </span>
            }
            description={`${m.width}×${m.height} · масштаб ${Math.round(m.scale * 100)}% · позиция ${m.x}, ${m.y}`}
          >
            <Button small onClick={() => run(() => ipc.moveToMonitor(m.key))}>Переместить сюда</Button>
          </Field>
        ))}
        {pos.monitor && !monitors.data?.some((m) => m.key === pos.monitor) && <Note kind="warn">Выбранный монитор «{pos.monitor}» сейчас не подключён — панель временно на основном.</Note>}
      </Group>
      {error && <Note kind="error">{error}</Note>}
    </Page>
  );
}

export function SecuritySection() {
  const s = useApp((x) => x.settings!);
  const update = useApp((x) => x.update);
  const { error, run } = useAction();
  return (
    <Page title="Безопасность" description="Desktop Island работает локально и не отправляет данные на внешние серверы.">
      <Group>
        <Field label="Разрешить плагины" description="Главный выключатель: при отключении ни один плагин не запускается.">
          <Toggle checked={s.security.pluginsEnabled} onChange={(v) => run(() => update("security", { pluginsEnabled: v }))} />
        </Field>
        <Field label="Записывать системные действия в журнал" description="Включение Wi-Fi/Bluetooth, выключение компьютера и т.п. (без персональных данных).">
          <Toggle checked={s.security.logSystemActions} onChange={(v) => run(() => update("security", { logSystemActions: v }))} />
        </Field>
        <Field label="История буфера обмена">
          <Toggle checked={s.clipboard.historyEnabled} onChange={(v) => run(() => update("clipboard", { historyEnabled: v }))} />
        </Field>
        <Field label="Фильтр чувствительных данных в буфере обмена">
          <Toggle checked={s.clipboard.ignoreSensitive} onChange={(v) => run(() => update("clipboard", { ignoreSensitive: v }))} />
        </Field>
      </Group>
      <Group title="Как защищено приложение">
        <ul className="bullets">
          <li>IPC-команды доступны только локальным окнам панели и настроек; веб-окна нейросетей и плагины не имеют доступа к IPC.</li>
          <li>Все входные данные команд проверяются в Rust (диапазоны, форматы, пути, URL).</li>
          <li>Выключение и перезагрузка требуют подтверждения и одноразового токена.</li>
          <li>Плагины работают в песочнице без сети и файловой системы; каждое разрешение подтверждается пользователем.</li>
          <li>Веб-версии нейросетей используют изолированные каталоги данных — cookies сервисов не смешиваются.</li>
          <li>Системные настройки прокси и VPN не изменяются — только чтение состояния.</li>
          <li>Журнал событий не содержит текста уведомлений и содержимого буфера обмена.</li>
        </ul>
      </Group>
      {error && <Note kind="error">{error}</Note>}
    </Page>
  );
}

const CATS: [string, string][] = [
  ["", "Все"],
  ["app", "Приложение"],
  ["module", "Модули"],
  ["integration", "Интеграции"],
  ["plugin", "Плагины"],
  ["system_action", "Системные действия"],
  ["permission", "Разрешения"],
  ["game_mode", "Игровой режим"],
  ["database", "База данных"],
];

export function LogsSection() {
  const [cat, setCat] = useState("");
  const [lvl, setLvl] = useState("");
  const logs = useLoad(() => ipc.logs(500, cat || null, lvl || null), [cat, lvl]);
  const logging = useApp((s) => s.settings!.logging);
  const update = useApp((s) => s.update);
  const { error, run } = useAction();
  return (
    <Page
      title="Журнал событий"
      description="Запуск и завершение, ошибки модулей и интеграций, плагины, изменения системных настроек, ошибки разрешений, игровой режим."
      actions={
        <>
          <Button onClick={() => run(() => ipc.openPath("logs"))}>
            <FolderOpen size={14} /> Файлы логов
          </Button>
          <ConfirmButton label="Очистить" title="Очистить журнал событий?" text="Все записи журнала будут удалены." onConfirm={() => run(async () => { await ipc.logsClear(); logs.reload(); })} />
        </>
      }
    >
      <Group>
        <Field label="Максимум записей">
          <NumberInput value={logging.maxEntries} min={100} max={50000} step={100} onChange={(v) => run(() => update("logging", { maxEntries: v }))} />
        </Field>
        <Field label="Уровень">
          <select className="input" value={logging.level} onChange={(e) => run(() => update("logging", { level: e.target.value }))}>
            <option value="error">Только ошибки</option>
            <option value="warn">Предупреждения</option>
            <option value="info">Информация</option>
            <option value="debug">Отладка</option>
          </select>
        </Field>
      </Group>
      <div className="row gap" style={{ margin: "8px 0" }}>
        <select className="input" value={cat} onChange={(e) => setCat(e.target.value)}>
          {CATS.map(([v, l]) => (
            <option key={v} value={v}>{l}</option>
          ))}
        </select>
        <select className="input" value={lvl} onChange={(e) => setLvl(e.target.value)}>
          <option value="">Все уровни</option>
          <option value="error">Ошибки</option>
          <option value="warn">Предупреждения</option>
          <option value="info">Информация</option>
        </select>
        <Button onClick={logs.reload}>
          <RefreshCw size={13} />
        </Button>
      </div>
      <div className="log-list">
        {logs.data?.length === 0 && <div className="muted small">Записей нет</div>}
        {logs.data?.map((l) => (
          <div key={l.id} className={`log-row lvl-${l.level}`}>
            <span className="mono small muted">{formatDateTime(l.ts)}</span>
            <span className={`pill lvl-${l.level}`}>{l.level}</span>
            <span className="small muted">{l.category}/{l.source}</span>
            <span className="grow">{l.message}</span>
          </div>
        ))}
      </div>
      {error && <Note kind="error">{error}</Note>}
    </Page>
  );
}

export function AboutSection() {
  const about = useLoad(() => ipc.about());
  const env = useApp((s) => s.environment);
  if (!about.data) return <Spinner />;
  const a = about.data;
  return (
    <Page title="О программе" description="Desktop Island — лёгкая расширяемая системная панель для Windows и Linux.">
      <Group>
        <Field label="Версия" description={`${a.version} · ${a.os} ${a.arch} · Plugin API v${a.pluginApi}`} />
        <Field label="Система" description={`${env?.osVersion ?? ""}${env?.desktop ? ` · ${env.desktop}` : ""}${env?.sessionType ? ` · ${env.sessionType}` : ""}`} />
        <Field label="Автозапуск" description={a.autostart ? "Включён" : "Выключен"} />
      </Group>
      {env && env.notes.length > 0 && (
        <Group title="Ограничения текущего окружения">
          <ul className="bullets">
            {env.notes.map((n) => (
              <li key={n}>{n}</li>
            ))}
          </ul>
        </Group>
      )}
    </Page>
  );
}
