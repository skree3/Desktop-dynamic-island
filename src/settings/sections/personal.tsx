import { buildPalettes } from "../../utils/style";
// Разделы: Профили, Темы, Горячие клавиши.

import { useEffect, useRef, useState, type ReactNode } from "react";
import { Check, Copy, Download, FileUp, Pencil, Plus, Share2, Trash2 } from "lucide-react";
import { Button, Confirm, IconButton, Note, Toggle } from "../../components/ui";
import { useAction, useLoad } from "../../hooks/common";
import { useResolvedMode } from "../../hooks/useTheme";
import { ipc } from "../../services/ipc";
import { useApp } from "../../stores/app";
import type { HotkeyStatus, ProfileCondition, ProfileRule, ThemeRow } from "../../types";
import { ConfirmButton, Field, Group, Page } from "../components";

export function ProfilesSection() {
  const profiles = useLoad(() => ipc.profiles());
  const p = useApp((s) => s.settings!.profiles);
  const update = useApp((s) => s.update);
  const { error, run } = useAction();
  const [name, setName] = useState("");

  const setRules = (rules: ProfileRule[]) => run(() => update("profiles", { rules }));
  const addRule = (condition: ProfileCondition) =>
    setRules([...p.rules, { id: crypto.randomUUID(), condition, profileId: profiles.data?.[0]?.id ?? "normal", enabled: true }]);

  return (
    <Page title="Профили" description="Профиль хранит настройки карточек, уведомлений, положения панели, частоты обновления и включённые модули.">
      <Group title="Профили">
        {profiles.data?.map((pr) => (
          <Field
            key={pr.id}
            label={
              <span className="row gap">
                {pr.name}
                {p.active === pr.id && <span className="pill level-full">активен</span>}
                {pr.builtin && <span className="pill">встроенный</span>}
              </span>
            }
          >
            <div className="row gap">
              <Button small variant={p.active === pr.id ? "default" : "primary"} disabled={p.active === pr.id} onClick={() => run(() => ipc.profileApply(pr.id))}>
                {p.active === pr.id ? "Применено" : "Применить"}
              </Button>
              <Button small title="Записать текущие настройки в профиль" onClick={() => run(() => ipc.profileSaveCurrent(pr.id))}>
                Сохранить текущие
              </Button>
              <ConfirmButton small label={<Trash2 size={13} />} title={`Удалить профиль «${pr.name}»?`} text="Настройки профиля будут удалены." onConfirm={() => run(async () => { await ipc.profileDelete(pr.id); profiles.reload(); })} />
            </div>
          </Field>
        ))}
        <div className="row gap" style={{ marginTop: 8 }}>
          <input className="input grow" placeholder="Название нового профиля" value={name} maxLength={48} onChange={(e) => setName(e.target.value)} />
          <Button
            variant="primary"
            disabled={!name.trim()}
            onClick={() =>
              run(async () => {
                await ipc.profileCreate(name);
                setName("");
                profiles.reload();
              })
            }
          >
            <Plus size={14} /> Создать из текущих
          </Button>
        </div>
      </Group>

      <Group title="Автоматическое переключение">
        <Field label="Переключать профили автоматически" description="Когда условие перестаёт выполняться, возвращается предыдущий профиль.">
          <Toggle checked={p.autoSwitch} onChange={(v) => run(() => update("profiles", { autoSwitch: v }))} />
        </Field>
        {p.rules.map((r) => (
          <Field
            key={r.id}
            label={r.condition.kind === "gameMode" ? "Во время игрового режима" : r.condition.kind === "onBattery" ? "При работе от батареи" : "По времени"}
          >
            <div className="row gap">
              {r.condition.kind === "timeRange" && (
                <>
                  <input className="input" type="time" value={r.condition.from} onChange={(e) => setRules(p.rules.map((x) => (x.id === r.id ? { ...x, condition: { kind: "timeRange", from: e.target.value, to: (x.condition as any).to } } : x)))} />
                  —
                  <input className="input" type="time" value={r.condition.to} onChange={(e) => setRules(p.rules.map((x) => (x.id === r.id ? { ...x, condition: { kind: "timeRange", from: (x.condition as any).from, to: e.target.value } } : x)))} />
                </>
              )}
              <select className="input select-sm" value={r.profileId} onChange={(e) => setRules(p.rules.map((x) => (x.id === r.id ? { ...x, profileId: e.target.value } : x)))}>
                {profiles.data?.map((pr) => (
                  <option key={pr.id} value={pr.id}>
                    {pr.name}
                  </option>
                ))}
              </select>
              <Toggle checked={r.enabled} onChange={(v) => setRules(p.rules.map((x) => (x.id === r.id ? { ...x, enabled: v } : x)))} />
              <IconButton size="sm" title="Удалить правило" onClick={() => setRules(p.rules.filter((x) => x.id !== r.id))}>
                <Trash2 size={14} />
              </IconButton>
            </div>
          </Field>
        ))}
        <div className="row gap">
          <Button small onClick={() => addRule({ kind: "gameMode" })}>+ Игровой режим</Button>
          <Button small onClick={() => addRule({ kind: "onBattery" })}>+ От батареи</Button>
          <Button small onClick={() => addRule({ kind: "timeRange", from: "09:00", to: "18:00" })}>+ По времени</Button>
        </div>
      </Group>
      {error && <Note kind="error">{error}</Note>}
    </Page>
  );
}

const STYLE_DEFAULTS = { accent: "#f0a64a", base: "#2b4a6f", tint: "#c9a0ff" };
const isHex = (c: unknown): c is string => typeof c === "string" && /^#[0-9a-fA-F]{6}$/.test(c.trim());

/** Начальные значения редактора: из исходных цветов темы, а если их нет — из её акцента. */
function editorInit(t: ThemeRow | null) {
  if (!t) return { name: "Мой стиль", accent: STYLE_DEFAULTS.accent, base: STYLE_DEFAULTS.base, useBase: true, tint: STYLE_DEFAULTS.tint, useTint: false };
  const src = t.data.source;
  const accent = isHex(src?.accent) ? src!.accent : isHex(t.data.dark?.accent) ? t.data.dark.accent : STYLE_DEFAULTS.accent;
  return {
    name: t.name,
    accent,
    base: isHex(src?.base) ? src!.base! : STYLE_DEFAULTS.base,
    useBase: isHex(src?.base),
    tint: isHex(src?.tint) ? src!.tint! : STYLE_DEFAULTS.tint,
    useTint: isHex(src?.tint),
  };
}

function StyleEditor({ editing, onSaved, onCancel }: { editing: ThemeRow | null; onSaved: (id: string, edited: boolean) => void; onCancel: () => void }) {
  const init = editorInit(editing);
  const [name, setName] = useState(init.name);
  const [accent, setAccent] = useState(init.accent);
  const [useBase, setUseBase] = useState(init.useBase);
  const [base, setBase] = useState(init.base);
  const [useTint, setUseTint] = useState(init.useTint);
  const [tint, setTint] = useState(init.tint);
  const { error, run, busy } = useAction();
  const mode = useResolvedMode();
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (editing) ref.current?.scrollIntoView({ behavior: "smooth", block: "center" });
  }, [editing]);
  const pal = buildPalettes(accent, useBase ? base : null, useTint ? tint : null);
  const p = pal[mode];
  const save = () =>
    run(async () => {
      const id = editing?.id ?? `custom-${Date.now().toString(36)}`;
      const source = { accent, base: useBase ? base : null, tint: useTint ? tint : null };
      await ipc.themeSave(JSON.stringify({ id, name: name.trim() || "Мой стиль", dark: pal.dark, light: pal.light, source }));
      useApp.setState({ themes: await ipc.themes() });
      onSaved(id, !!editing);
    });
  return (
    <div ref={ref}>
      <Group title={editing ? `Изменение: ${editing.name}` : "Свой стиль"}>
        <div className="field wide">
          <div className="style-editor">
            <div className="style-colors">
              <label className="color-pick">
                <input type="color" value={accent} onChange={(e) => setAccent(e.target.value)} />
                <span>Акцент</span>
              </label>
              <label className="color-pick" style={{ marginTop: 10 }}>
                <input type="color" value={base} disabled={!useBase} onChange={(e) => setBase(e.target.value)} />
                <span className="grow">Оттенок фона</span>
                <Toggle checked={useBase} label="Оттенок фона" onChange={setUseBase} />
              </label>
              <label className="color-pick" style={{ marginTop: 10 }}>
                <input type="color" value={tint} disabled={!useTint} onChange={(e) => setTint(e.target.value)} />
                <span className="grow">Оттенок текста</span>
                <Toggle checked={useTint} label="Оттенок текста" onChange={setUseTint} />
              </label>
              <input className="input" style={{ marginTop: 14, width: "100%" }} value={name} maxLength={40} aria-label="Название стиля" onChange={(e) => setName(e.target.value)} />
              <div className="row gap" style={{ marginTop: 8 }}>
                <Button variant="primary" disabled={busy} onClick={save}>
                  {editing ? "Сохранить изменения" : "Сохранить и включить"}
                </Button>
                {editing && <Button onClick={onCancel}>Отмена</Button>}
              </div>
              {error && <Note kind="error">{error}</Note>}
            </div>
            <div className="style-preview" style={{ background: p.bg, color: p.text }}>
              <span className="pv-island" style={{ background: "#1c1c1f" }}>
                <i style={{ background: p.accent }} />
                <b style={{ background: "#ededed" }} />
              </span>
              <span className="pv-card" style={{ background: p.surface }}>
                Уведомления
                <span className="pv-tg" style={{ background: p.accent }}>
                  <style>{`.pv-tg::after{background:${p.accentText}}`}</style>
                </span>
              </span>
              <span className="pv-card" style={{ background: p.surface, color: p.textMuted }}>
                Музыка
              </span>
            </div>
          </div>
        </div>
      </Group>
    </div>
  );
}

function Modal({ title, children, onClose }: { title: string; children: ReactNode; onClose: () => void }) {
  useEffect(() => {
    const h = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, [onClose]);
  return (
    <>
      <div className="backdrop" onClick={onClose} />
      <div className="confirm theme-modal" role="dialog" aria-label={title}>
        <div className="confirm-title">{title}</div>
        {children}
      </div>
    </>
  );
}

async function copyText(text: string, field: HTMLTextAreaElement | null) {
  try {
    // writeText может зависнуть, если окно не в фокусе, — ждём не дольше 1,5 с.
    await Promise.race([navigator.clipboard.writeText(text), new Promise((_, rej) => setTimeout(() => rej(new Error("timeout")), 1500))]);
  } catch {
    field?.focus();
    field?.select();
    if (!document.execCommand("copy")) throw new Error("Не удалось скопировать — выделите код и нажмите Ctrl+C");
  }
}

function ShareDialog({ theme, onClose }: { theme: ThemeRow; onClose: () => void }) {
  const code = useLoad(() => ipc.themeShareCode(theme.id), [theme.id]);
  const field = useRef<HTMLTextAreaElement>(null);
  const [done, setDone] = useState<string | null>(null);
  const { error, run, busy } = useAction();
  return (
    <Modal title={`Поделиться темой «${theme.name}»`} onClose={onClose}>
      <div className="confirm-text">Отправьте код или файл. Получатель добавит тему кнопкой «Импорт темы».</div>
      <textarea ref={field} className="input theme-code" readOnly rows={4} value={code.data ?? ""} onFocus={(e) => e.currentTarget.select()} aria-label="Код темы" />
      {done && <Note kind="info">{done}</Note>}
      {(error || code.error) && <Note kind="error">{error || code.error}</Note>}
      <div className="confirm-actions">
        <Button disabled={busy} onClick={() => run(async () => { const p = await ipc.themeExportFile(theme.id); if (p) setDone(`Файл сохранён: ${p}`); })}>
          <Download size={14} /> В файл…
        </Button>
        <span className="grow" />
        <Button onClick={onClose}>Закрыть</Button>
        <Button variant="primary" disabled={!code.data} onClick={() => run(async () => { await copyText(code.data!, field.current); setDone("Код скопирован"); })}>
          <Copy size={14} /> Скопировать код
        </Button>
      </div>
    </Modal>
  );
}

function ImportDialog({ onDone, onClose }: { onDone: (t: ThemeRow) => void; onClose: () => void }) {
  const [text, setText] = useState("");
  const { error, run, busy } = useAction();
  const finish = async (t: ThemeRow | null) => {
    if (!t) return;
    useApp.setState({ themes: await ipc.themes() });
    onDone(t);
  };
  return (
    <Modal title="Импорт темы" onClose={onClose}>
      <div className="confirm-text">Вставьте код темы, который вам прислали, или выберите файл темы.</div>
      <textarea className="input theme-code" rows={4} autoFocus value={text} placeholder="DITHEME1:…" onChange={(e) => setText(e.target.value)} aria-label="Код темы" />
      {error && <Note kind="error">{error}</Note>}
      <div className="confirm-actions">
        <Button disabled={busy} onClick={() => run(async () => finish(await ipc.themeImportFile()))}>
          <FileUp size={14} /> Из файла…
        </Button>
        <span className="grow" />
        <Button onClick={onClose}>Отмена</Button>
        <Button variant="primary" disabled={busy || !text.trim()} onClick={() => run(async () => finish(await ipc.themeImport(text)))}>
          Добавить
        </Button>
      </div>
    </Modal>
  );
}

export function ThemesSection() {
  const themes = useApp((s) => s.themes);
  const a = useApp((s) => s.settings!.appearance);
  const update = useApp((s) => s.update);
  const mode = useResolvedMode();
  const { error, run } = useAction();
  const [editing, setEditing] = useState<ThemeRow | null>(null);
  const [sharing, setSharing] = useState<ThemeRow | null>(null);
  const [deleting, setDeleting] = useState<ThemeRow | null>(null);
  const [importing, setImporting] = useState(false);
  const [busyDel, setBusyDel] = useState(false);
  const [editorKey, setEditorKey] = useState(0);
  const apply = (id: string) => run(() => update("appearance", { themeId: id }));
  return (
    <Page
      title="Темы"
      description="Каждая тема содержит тёмную и светлую палитру. Режим выбирается в «Внешнем виде». Свои темы можно изменить, отправить другому человеку или удалить — кнопки появляются при наведении на карточку."
      actions={
        <Button onClick={() => setImporting(true)}>
          <FileUp size={14} /> Импорт темы
        </Button>
      }
    >
      <div className="theme-grid">
        {(themes ?? []).map((t) => {
          const pal = t.data[mode] ?? t.data.dark;
          const on = a.themeId === t.id;
          return (
            <div key={t.id} className={`theme-card ${on ? "on" : ""} ${editing?.id === t.id ? "editing" : ""}`} onClick={() => apply(t.id)}>
              <div className="theme-preview" style={{ background: pal.bg, color: pal.text }}>
                <span className="theme-pill" style={{ background: "#000", boxShadow: `0 0 0 1px ${pal.border}` }}>
                  <i style={{ background: pal.accent }} />
                  <b style={{ background: pal.text }} />
                </span>
                <span className="theme-surface" style={{ background: pal.surface }} />
                {!t.builtin && (
                  <div className="theme-actions">
                    <IconButton size="sm" title="Изменить" onClick={() => setEditing(t)}>
                      <Pencil size={13} />
                    </IconButton>
                    <IconButton size="sm" title="Поделиться" onClick={() => setSharing(t)}>
                      <Share2 size={13} />
                    </IconButton>
                    <IconButton size="sm" title="Удалить" onClick={() => setDeleting(t)}>
                      <Trash2 size={13} />
                    </IconButton>
                  </div>
                )}
              </div>
              <div className="row between">
                <span className="theme-name">{t.name}</span>
                {on && <Check size={14} className="accent" />}
              </div>
            </div>
          );
        })}
      </div>
      {error && <Note kind="error">{error}</Note>}
      <StyleEditor
        key={editing?.id ?? `new-${editorKey}`}
        editing={editing}
        onCancel={() => setEditing(null)}
        onSaved={(id, edited) => {
          setEditing(null);
          setEditorKey((k) => k + 1);
          if (!edited) apply(id);
        }}
      />
      {sharing && <ShareDialog theme={sharing} onClose={() => setSharing(null)} />}
      {importing && (
        <ImportDialog
          onClose={() => setImporting(false)}
          onDone={(t) => {
            setImporting(false);
            apply(t.id);
          }}
        />
      )}
      {deleting && (
        <>
          <div className="backdrop" onClick={() => !busyDel && setDeleting(null)} />
          <Confirm
            title={`Удалить тему «${deleting.name}»?`}
            text={a.themeId === deleting.id ? "Тема включена — после удаления включится «Полночь». Вернуть тему можно только из кода или файла." : "Вернуть тему можно только из кода или файла, если вы им делились."}
            confirmLabel="Удалить"
            danger
            busy={busyDel}
            onCancel={() => setDeleting(null)}
            onConfirm={async () => {
              setBusyDel(true);
              const id = deleting.id;
              await run(async () => {
                await ipc.themeDelete(id);
                useApp.setState({ themes: await ipc.themes() });
              });
              if (editing?.id === id) setEditing(null);
              setBusyDel(false);
              setDeleting(null);
            }}
          />
        </>
      )}
    </Page>
  );
}

function acceleratorFrom(e: KeyboardEvent): string | null {
  const mods: string[] = [];
  if (e.ctrlKey) mods.push("Ctrl");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  if (e.metaKey) mods.push("Super");
  const k = e.code;
  let key: string | null = null;
  if (/^Key[A-Z]$/.test(k)) key = k.slice(3);
  else if (/^Digit\d$/.test(k)) key = k.slice(5);
  else if (/^F\d{1,2}$/.test(k)) key = k;
  else if (["Space", "Enter", "Tab", "Backspace", "Delete", "Insert", "Home", "End", "PageUp", "PageDown"].includes(k)) key = k;
  else if (k.startsWith("Arrow")) key = k.slice(5);
  if (!key || mods.length === 0) return null;
  return [...mods, key].join("+");
}

export function HotkeysSection() {
  const list = useLoad(() => ipc.hotkeys());
  const env = useApp((s) => s.environment);
  const [recording, setRecording] = useState<string | null>(null);
  const { error, run } = useAction();

  useEffect(() => {
    if (!recording) return;
    ipc.hotkeysSuspend(true).catch(() => {});
    const h = (e: KeyboardEvent) => {
      e.preventDefault();
      if (e.key === "Escape") return setRecording(null);
      const acc = acceleratorFrom(e);
      if (!acc) return;
      const cur = list.data?.find((x) => x.action === recording);
      run(async () => list.setData(await ipc.hotkeySet(recording, acc, cur?.enabled ?? true)));
      setRecording(null);
    };
    window.addEventListener("keydown", h);
    return () => {
      window.removeEventListener("keydown", h);
      ipc.hotkeysSuspend(false).then(() => list.reload()).catch(() => {});
    };
  }, [recording]);

  return (
    <Page title="Горячие клавиши" description="Глобальные сочетания работают, даже когда панель скрыта." actions={<Button onClick={() => run(async () => list.setData(await ipc.hotkeysReset()))}>Сбросить</Button>}>
      {env && !env.globalHotkeysSupported && (
        <Note kind="warn">
          В этом окружении (Wayland) приложения не могут перехватывать клавиши глобально. Назначьте сочетания в настройках окружения рабочего стола на команды:
          <code> desktop-island --toggle</code>, <code>--search</code>, <code>--quick</code>, <code>--dnd</code>, <code>--play-pause</code>.
        </Note>
      )}
      <Group>
        {list.data?.map((h: HotkeyStatus) => (
          <Field key={h.action} label={h.label} description={h.error ?? (h.registered ? "Зарегистрировано" : h.enabled ? "Не назначено" : "Отключено")}>
            <div className="row gap">
              <button type="button" className={`kbd ${recording === h.action ? "rec" : ""}`} onClick={() => setRecording(recording === h.action ? null : h.action)}>
                {recording === h.action ? "Нажмите сочетание… (Esc — отмена)" : h.accelerator || "—"}
              </button>
              <Toggle checked={h.enabled} onChange={(v) => run(async () => list.setData(await ipc.hotkeySet(h.action, h.accelerator, v)))} />
            </div>
          </Field>
        ))}
      </Group>
      {error && <Note kind="error">{error}</Note>}
    </Page>
  );
}
