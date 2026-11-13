// Разделы: Карточки, Уведомления, Музыка, Системный монитор, Буфер обмена.

import { useState } from "react";
import { ArrowDown, ArrowUp, Trash2 } from "lucide-react";
import { Button, IconButton, Note, Toggle } from "../../components/ui";
import { useAction, useLoad } from "../../hooks/common";
import { ipc } from "../../services/ipc";
import { moduleKey, useApp } from "../../stores/app";
import type { AppRule, ModuleId, Priority } from "../../types";
import { ConfirmButton, Field, Group, ListEditor, NumberInput, Page } from "../components";

const EXTRA_MODULES: { id: ModuleId; label: string; desc: string }[] = [
  { id: "volume", label: "Громкость", desc: "Регулировка в быстрых настройках и карточке музыки" },
  { id: "brightness", label: "Яркость", desc: "Регулировка яркости дисплеев" },
  { id: "power", label: "Выключение и перезагрузка", desc: "Кнопки в быстрых настройках (с подтверждением)" },
  { id: "battery", label: "Зарядка и низкий заряд", desc: "Анимация при подключении зарядки и предупреждения при 20% и 10%" },
  { id: "privacy", label: "Микрофон, камера и звонки", desc: "Точки, когда приложение использует микрофон или камеру; звонок с длительностью и кнопкой микрофона" },
  { id: "downloads", label: "Загрузки", desc: "Скачиваемый файл и скорость в острове, уведомление по окончании" },
];

export function CardsSection() {
  const cards = useApp((s) => s.cards);
  const modules = useApp((s) => s.settings!.modules);
  const update = useApp((s) => s.update);
  const { error, run } = useAction();

  const save = (list: typeof cards) =>
    run(async () => {
      const next = await ipc.updateCards(list.map((c, i) => ({ cardId: c.id, enabled: c.enabled, sortOrder: i, priority: c.priority })));
      useApp.setState({ cards: next });
    });
  const move = (i: number, d: number) => {
    const list = [...cards];
    const j = i + d;
    if (j < 0 || j >= list.length) return;
    [list[i], list[j]] = [list[j], list[i]];
    save(list);
  };

  return (
    <Page
      title="Карточки"
      description="«В панели» — показывать ли панель в острове (листаются колесом мыши). «Модуль» — работает ли функция вообще: отключённый модуль не выполняет фоновую работу и не занимает память."
    >
      <Group title="Порядок и видимость">
        {cards.map((c, i) => (
          <div key={c.id} className="field">
            <div className="field-text">
              <div className="field-label">{c.title}</div>
              <div className="field-desc">{c.pluginId ? `Плагин ${c.pluginId}` : `Модуль: ${c.moduleEnabled ? "включён" : "выключен"}`}</div>
            </div>
            <div className="field-control row gap">
              <select
                className="input select-sm"
                value={c.priority}
                title="Приоритет"
                onChange={(e) => save(cards.map((x) => (x.id === c.id ? { ...x, priority: Number(e.target.value) } : x)))}
              >
                <option value={0}>Низкий</option>
                <option value={1}>Обычный</option>
                <option value={2}>Высокий</option>
              </select>
              <IconButton size="sm" title="Выше" disabled={i === 0} onClick={() => move(i, -1)}>
                <ArrowUp size={14} />
              </IconButton>
              <IconButton size="sm" title="Ниже" disabled={i === cards.length - 1} onClick={() => move(i, 1)}>
                <ArrowDown size={14} />
              </IconButton>
              <span className="small muted">В панели</span>
              <Toggle label="Показывать в панели" checked={c.enabled} onChange={(v) => save(cards.map((x) => (x.id === c.id ? { ...x, enabled: v } : x)))} />
              {!c.pluginId && (
                <>
                  <span className="small muted">Модуль</span>
                  <Toggle
                    label="Модуль"
                    checked={c.moduleEnabled}
                    onChange={(v) => run(() => update("modules", { [moduleKey(c.module)]: v } as any))}
                  />
                </>
              )}
            </div>
          </div>
        ))}
      </Group>
      <Group title="Дополнительные модули">
        {EXTRA_MODULES.map((m) => (
          <Field key={m.id} label={m.label} description={m.desc}>
            <Toggle checked={modules[m.id]} onChange={(v) => run(() => update("modules", { [m.id]: v } as any))} />
          </Field>
        ))}
      </Group>
      {error && <Note kind="error">{error}</Note>}
    </Page>
  );
}

export function NotificationsSection() {
  const n = useApp((s) => s.settings!.notifications);
  const update = useApp((s) => s.update);
  const status = useLoad(() => ipc.listenerStatus());
  const { error, run } = useAction();
  const [app, setApp] = useState("");
  const set = (p: Partial<typeof n>) => run(() => update("notifications", p));
  const setRule = (rule: AppRule) => set({ appRules: n.appRules.map((r) => (r.app === rule.app ? rule : r)) });

  return (
    <Page title="Уведомления" description="Event Manager показывает уведомления по одному, по приоритету, без наложения на другие карточки.">
      <Group title="Источник">
        <Field label="Показывать уведомления в панели">
          <Toggle checked={n.enabled} onChange={(v) => set({ enabled: v })} />
        </Field>
        <Field label="Получать уведомления других приложений" description={status.data ? `${status.data.backend}: ${status.data.message}` : "Проверка…"}>
          <Toggle checked={n.listenSystem} onChange={(v) => set({ listenSystem: v })} />
        </Field>
        <Field
          label="Только в острове"
          description="Баннеры и звук Windows приглушаются режимом «Не беспокоить», уведомления приходят только в остров. После выхода из программы всё возвращается как было. Если Telegram присылает только «Новое сообщение», выключите в его настройках уведомлений учёт режима фокусировки"
        >
          <Toggle checked={n.islandOnly} disabled={!n.listenSystem} onChange={(v) => set({ islandOnly: v })} />
        </Field>
        <Field label="Звук уведомлений" description="Короткий тихий сигнал острова">
          <Toggle checked={n.sound ?? true} onChange={(v) => set({ sound: v })} />
        </Field>
        <Field label="Группировать по приложениям" description="Несколько сообщений одного приложения — одна карточка со счётчиком">
          <Toggle checked={n.groupByApp ?? true} onChange={(v) => set({ groupByApp: v })} />
        </Field>
        {status.data && !status.data.available && (
          <Note kind="warn">
            {status.data.message}
            <div style={{ marginTop: 6 }}>
              <Button small onClick={() => run(async () => { await ipc.listenerRetry(); setTimeout(status.reload, 1500); })}>Повторить попытку</Button>
            </div>
          </Note>
        )}
      </Group>
      <SourcesGroup />
      <Group title="Не беспокоить">
        <Field label="Режим «Не беспокоить»" description="Уведомления сохраняются в истории, но не всплывают.">
          <Toggle checked={n.dnd} onChange={(v) => set({ dnd: v })} />
        </Field>
        <Field label="Пропускать критические" description="Например, окончание таймера с критическим приоритетом.">
          <Toggle checked={n.dndAllowCritical} onChange={(v) => set({ dndAllowCritical: v })} />
        </Field>
      </Group>
      <Group title="Отображение">
        <Field label="Длительность показа">
          <NumberInput value={n.durationSecs} min={2} max={60} suffix="с" onChange={(v) => set({ durationSecs: v })} />
        </Field>
        <Field label="Размер истории" description="История хранится только в памяти и очищается при перезапуске.">
          <NumberInput value={n.historyLimit} min={0} max={500} onChange={(v) => set({ historyLimit: v })} />
        </Field>
        <Field label="Показывать текст сообщений" description="Если выключено — только название приложения и заголовок.">
          <Toggle checked={n.showBody} onChange={(v) => set({ showBody: v })} />
        </Field>
        <Field label="Проверка">
          <div className="row gap">
            <Button onClick={() => run(() => ipc.notificationTest("normal"))}>Обычное</Button>
            <Button onClick={() => run(() => ipc.notificationTest("critical"))}>Критическое</Button>
          </div>
        </Field>
      </Group>
      <Group title="Приложения: приоритеты и отключение">
        <div className="row gap" style={{ marginBottom: 8 }}>
          <input className="input grow" placeholder="Название приложения (как в уведомлении), например Telegram" value={app} onChange={(e) => setApp(e.target.value)} />
          <Button
            onClick={() => {
              const v = app.trim();
              if (v && !n.appRules.some((r) => r.app.toLowerCase() === v.toLowerCase())) set({ appRules: [...n.appRules, { app: v, muted: false, priority: null }] });
              setApp("");
            }}
          >
            Добавить
          </Button>
        </div>
        {n.appRules.map((r) => (
          <Field key={r.app} label={r.app}>
            <div className="row gap">
              <select className="input select-sm" value={r.priority ?? ""} onChange={(e) => setRule({ ...r, priority: (e.target.value || null) as Priority | null })}>
                <option value="">Приоритет по умолчанию</option>
                <option value="low">Низкий</option>
                <option value="normal">Обычный</option>
                <option value="high">Высокий</option>
                <option value="critical">Критический</option>
              </select>
              <span className="small muted">Отключить</span>
              <Toggle checked={r.muted} onChange={(v) => setRule({ ...r, muted: v })} />
              <IconButton size="sm" title="Удалить правило" onClick={() => set({ appRules: n.appRules.filter((x) => x.app !== r.app) })}>
                <Trash2 size={14} />
              </IconButton>
            </div>
          </Field>
        ))}
      </Group>
      {error && <Note kind="error">{error}</Note>}
    </Page>
  );
}

/** Популярные приложения: уведомления от них приходят автоматически, если приложение
 *  показывает их через систему (Windows-уведомления или D-Bus в Linux). */
const KNOWN_SOURCES: { label: string; names: string[] }[] = [
  { label: "Telegram", names: ["Telegram", "Telegram Desktop", "AyuGram"] },
  { label: "Discord", names: ["Discord"] },
  { label: "Steam", names: ["Steam"] },
  { label: "WhatsApp", names: ["WhatsApp"] },
  { label: "Почта и Outlook", names: ["Почта", "Mail", "Outlook", "Thunderbird"] },
  { label: "Microsoft Teams", names: ["Microsoft Teams", "Teams"] },
  { label: "Viber", names: ["Viber"] },
  { label: "Slack", names: ["Slack"] },
];

function SourcesGroup() {
  const n = useApp((s) => s.settings!.notifications);
  const update = useApp((s) => s.update);
  const history = useLoad(() => ipc.notificationsHistory());
  const isMuted = (names: string[]) => names.some((nm) => n.appRules.some((r) => r.muted && r.app.toLowerCase() === nm.toLowerCase()));
  const setMuted = (names: string[], muted: boolean) => {
    const lower = names.map((x) => x.toLowerCase());
    const rules = n.appRules.filter((r) => !lower.includes(r.app.toLowerCase()));
    if (muted) for (const nm of names) rules.push({ app: nm, muted: true, priority: null });
    update("notifications", { appRules: rules }).catch(() => {});
  };
  const knownNames = KNOWN_SOURCES.flatMap((s) => s.names.map((x) => x.toLowerCase()));
  const seen = [...new Set((history.data ?? []).filter((h) => h.kind === "system").map((h) => h.appName))].filter((a) => !knownNames.includes(a.toLowerCase()));
  return (
    <Group title="Приложения">
      <div className="small muted" style={{ margin: "0 0 6px 2px" }}>
        Уведомления подключаются автоматически: всё, что приложение показывает как системное уведомление, появится в острове. Выключите те, что не нужны.
      </div>
      {[...KNOWN_SOURCES, ...seen.map((a) => ({ label: a, names: [a] }))].map((s) => (
        <Field key={s.label} label={s.label}>
          <Toggle checked={!isMuted(s.names)} onChange={(v) => setMuted(s.names, !v)} />
        </Field>
      ))}
    </Group>
  );
}

export function MusicSection() {
  const m = useApp((s) => s.settings!.music);
  const update = useApp((s) => s.update);
  const set = (p: Partial<typeof m>) => update("music", p).catch(() => {});
  return (
    <Page title="Музыка" description="Управление через системные медиасессии: Windows SMTC и Linux MPRIS.">
      <Group>
        <Field label="Показывать обложку">
          <Toggle checked={m.showArtwork} onChange={(v) => set({ showArtwork: v })} />
        </Field>
        <Field label="Показывать трек в компактной панели">
          <Toggle checked={m.showInCompact} onChange={(v) => set({ showInCompact: v })} />
        </Field>
        <Field label="Текст песни в свёрнутом острове" description="Строка песни вместо названия трека. Исполнитель и название отправляются в открытую базу lrclib.net.">
          <Toggle checked={m.lyrics} onChange={(v) => set({ lyrics: v })} />
        </Field>
        <Field label="Частота опроса медиасессий" description="События отправляются в интерфейс только при изменении трека или статуса.">
          <NumberInput value={m.pollIntervalMs} min={500} max={5000} step={100} suffix="мс" onChange={(v) => set({ pollIntervalMs: v })} />
        </Field>
      </Group>
    </Page>
  );
}

export function SystemMonitorSection() {
  const m = useApp((s) => s.settings!.systemMonitor);
  const update = useApp((s) => s.update);
  const set = (p: Partial<typeof m>) => update("systemMonitor", p).catch(() => {});
  return (
    <Page title="Системный монитор" description="Опрос датчиков работает только когда карточка открыта или включены компактные показатели.">
      <Group>
        <Field label="Интервал обновления">
          <NumberInput value={m.intervalMs} min={1000} max={30000} step={500} suffix="мс" onChange={(v) => set({ intervalMs: v })} />
        </Field>
        <Field label="Показывать загрузку CPU в компактной панели" description="Включает постоянный фоновый опрос.">
          <Toggle checked={m.showInCompact} onChange={(v) => set({ showInCompact: v })} />
        </Field>
        <Field label="Точек на графиках">
          <NumberInput value={m.historyPoints} min={10} max={300} onChange={(v) => set({ historyPoints: v })} />
        </Field>
      </Group>
    </Page>
  );
}

export function ClipboardSection() {
  const c = useApp((s) => s.settings!.clipboard);
  const env = useApp((s) => s.environment);
  const update = useApp((s) => s.update);
  const { error, run } = useAction();
  const set = (p: Partial<typeof c>) => run(() => update("clipboard", p));
  return (
    <Page title="Буфер обмена" description="История текста, изображений и файлов. Данные хранятся только локально.">
      {env?.sessionType === "wayland" && <Note kind="warn">Wayland: фоновое чтение буфера обмена работает только при поддержке протокола data-control (KDE Plasma, wlroots). В GNOME — частично через XWayland.</Note>}
      <Group>
        <Field label="Сохранять историю" description="Если выключено, буфер обмена не читается вообще.">
          <Toggle checked={c.historyEnabled} onChange={(v) => set({ historyEnabled: v })} />
        </Field>
        <Field label="Максимум записей" description="Закреплённые записи не удаляются.">
          <NumberInput value={c.maxItems} min={5} max={1000} onChange={(v) => set({ maxItems: v })} />
        </Field>
        <Field label="Сохранять изображения">
          <Toggle checked={c.storeImages} onChange={(v) => set({ storeImages: v })} />
        </Field>
        <Field label="Сохранять списки файлов">
          <Toggle checked={c.storeFiles} onChange={(v) => set({ storeFiles: v })} />
        </Field>
      </Group>
      <Group title="Безопасность">
        <Field
          label="Не сохранять чувствительные данные"
          description="Пропускает данные, помеченные менеджерами паролей (Windows), а также строки, похожие на пароли, API-ключи, токены и номера карт."
        >
          <Toggle checked={c.ignoreSensitive} onChange={(v) => set({ ignoreSensitive: v })} />
        </Field>
        <Field label="Исключённые приложения" description="Имя исполняемого файла, например keepassxc.exe" wide>
          <ListEditor items={c.excludedApps} placeholder="program.exe" onChange={(v) => set({ excludedApps: v })} />
        </Field>
        <Field label="Очистить историю">
          <ConfirmButton
            label="Удалить всё"
            title="Удалить всю историю буфера обмена?"
            text="Будут удалены все записи, включая закреплённые. Действие нельзя отменить."
            onConfirm={() => run(() => ipc.clipboardClear(false))}
          />
        </Field>
      </Group>
      {error && <Note kind="error">{error}</Note>}
    </Page>
  );
}
