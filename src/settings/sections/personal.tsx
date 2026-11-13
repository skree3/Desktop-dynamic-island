import { buildPalettes } from "../../utils/style";
// Разделы: Профили, Темы, Горячие клавиши.

import { useEffect, useState } from "react";
import { Check, Plus, Trash2 } from "lucide-react";
import { Button, IconButton, Note, Toggle } from "../../components/ui";
import { useAction, useLoad } from "../../hooks/common";
import { useResolvedMode } from "../../hooks/useTheme";
import { ipc } from "../../services/ipc";
import { useApp } from "../../stores/app";
import type { HotkeyStatus, ProfileCondition, ProfileRule } from "../../types";
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

function StyleEditor({ onSaved }: { onSaved: (id: string) => void }) {
  const [name, setName] = useState("Мой стиль");
  const [accent, setAccent] = useState("#f0a64a");
  const [useBase, setUseBase] = useState(true);
  const [base, setBase] = useState("#2b4a6f");
  const [useTint, setUseTint] = useState(false);
  const [tint, setTint] = useState("#c9a0ff");
  const { error, run, busy } = useAction();
  const mode = useResolvedMode();
  const pal = buildPalettes(accent, useBase ? base : null, useTint ? tint : null);
  const p = pal[mode];
  const save = () =>
    run(async () => {
      const id = `custom-${Date.now().toString(36)}`;
      await ipc.themeSave(JSON.stringify({ id, name: name.trim() || "Мой стиль", dark: pal.dark, light: pal.light }));
      useApp.setState({ themes: await ipc.themes() });
      onSaved(id);
    });
  return (
    <Group title="Свой стиль">
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
            <div className="row gap" style={{ marginTop: 14 }}>
              <input className="input grow" value={name} maxLength={40} aria-label="Название стиля" onChange={(e) => setName(e.target.value)} />
              <Button variant="primary" disabled={busy} onClick={save}>
                Сохранить и включить
              </Button>
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
  );
}

export function ThemesSection() {
  const themes = useApp((s) => s.themes);
  const a = useApp((s) => s.settings!.appearance);
  const update = useApp((s) => s.update);
  const mode = useResolvedMode();
  const { error, run } = useAction();
  return (
    <Page
      title="Темы"
      description="Каждая тема содержит тёмную и светлую палитру. Режим выбирается в «Внешнем виде»."
    >
      <div className="theme-grid">
        {themes.map((t) => {
          const pal = t.data[mode] ?? t.data.dark;
          const on = a.themeId === t.id;
          return (
            <div key={t.id} className={`theme-card ${on ? "on" : ""}`} onClick={() => run(() => update("appearance", { themeId: t.id }))}>
              <div className="theme-preview" style={{ background: pal.bg, color: pal.text }}>
                <span className="theme-pill" style={{ background: "#000", boxShadow: `0 0 0 1px ${pal.border}` }}>
                  <i style={{ background: pal.accent }} />
                  <b style={{ background: pal.text }} />
                </span>
                <span className="theme-surface" style={{ background: pal.surface }} />
              </div>
              <div className="row between">
                <span>{t.name}</span>
                {on && <Check size={14} className="accent" />}
              </div>
            </div>
          );
        })}
      </div>
      {error && <Note kind="error">{error}</Note>}
      <StyleEditor onSaved={(id) => run(() => update("appearance", { themeId: id }))} />
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
