// Разделы 1.1: Звуки, Шрифты, VPN, Индикаторы, Об авторе.

import { useEffect, useMemo, useState } from "react";
import { Check, FolderOpen, Pause, Play, RotateCcw, Trash2, Upload } from "lucide-react";
import { Button, IconButton, Note, Segmented, Slider, Spinner, Toggle } from "../../components/ui";
import { useAction, useLoad } from "../../hooks/common";
import { errorMessage, ipc } from "../../services/ipc";
import { useApp } from "../../stores/app";
import type { SoundSettings } from "../../types";
import { FONTS, loadCustomFont, resolveFont, verifyCustomFont } from "../../utils/fonts";
import { NOTIFICATION_SOUNDS, TIMER_SOUNDS, playSound, stopSound, type SoundDef } from "../../utils/sounds";
import { Field, Group, Page } from "../components";
import { AUTHOR } from "../author";

// ---------------------------------------------------------------- Звуки

function SoundGrid({ list, value, volume, loop, onPick }: { list: SoundDef[]; value: string; volume: number; loop?: boolean; onPick: (id: string) => void }) {
  const [playing, setPlaying] = useState<string | null>(null);
  useEffect(() => () => stopSound("preview"), []);
  const preview = (id: string) => {
    if (playing === id) {
      stopSound("preview");
      setPlaying(null);
      return;
    }
    setPlaying(id);
    playSound("preview", id, volume, { loop, limitSecs: loop ? 4 : undefined, onEnd: () => setPlaying((p) => (p === id ? null : p)) });
  };
  return (
    <div className="sound-grid">
      {list.map((s) => (
        <div key={s.id} className={`sound-chip ${value === s.id ? "on" : ""}`}>
          <button type="button" className="sound-pick" onClick={() => onPick(s.id)} aria-pressed={value === s.id}>
            {value === s.id && <Check size={13} />}
            <span>{s.label}</span>
          </button>
          <button type="button" className="sound-play" onClick={() => preview(s.id)} aria-label={playing === s.id ? `Остановить «${s.label}»` : `Прослушать «${s.label}»`}>
            {playing === s.id ? <Pause size={13} /> : <Play size={13} />}
          </button>
        </div>
      ))}
    </div>
  );
}

function CustomSound({ kind, value, volume }: { kind: "notification" | "timer"; value: string; volume: number }) {
  const { error, run, busy } = useAction();
  const custom = value.startsWith("custom:");
  return (
    <>
      <Field label="Свой звук" description={custom ? value.slice(7) : "WAV, MP3 или OGG до 5 МБ"}>
        <div className="row gap">
          {custom && (
            <IconButton size="sm" title="Прослушать" onClick={() => playSound("preview", value, volume, { limitSecs: 5 })}>
              <Play size={13} />
            </IconButton>
          )}
          <Button small disabled={busy} onClick={() => run(() => ipc.soundPick(kind))}>
            <FolderOpen size={14} /> {custom ? "Заменить" : "Выбрать файл"}
          </Button>
        </div>
      </Field>
      {error && <Note kind="error">{error}</Note>}
    </>
  );
}

const LIMITS = [
  { value: "15", label: "15 с" },
  { value: "30", label: "30 с" },
  { value: "60", label: "1 мин" },
  { value: "120", label: "2 мин" },
  { value: "0", label: "До остановки" },
];

export function SoundsSection() {
  const s = useApp((x) => x.settings!.sounds);
  const notifOn = useApp((x) => x.settings!.notifications.sound);
  const update = useApp((x) => x.update);
  const set = (patch: Partial<SoundSettings>) => update("sounds", patch).catch(() => {});
  const [nv, setNv] = useState(Math.round(s.notificationVolume * 100));
  const [tv, setTv] = useState(Math.round(s.timerVolume * 100));
  return (
    <Page title="Звуки">
      <Group title="Уведомления">
        <Field label="Звук уведомлений">
          <Toggle checked={notifOn} onChange={(sound) => update("notifications", { sound }).catch(() => {})} />
        </Field>
        <Field label="Сигнал" wide>
          <SoundGrid list={NOTIFICATION_SOUNDS} value={s.notification} volume={nv / 100} onPick={(notification) => { set({ notification }); playSound("preview", notification, nv / 100); }} />
        </Field>
        <Field label="Громкость">
          <Slider label="Громкость уведомлений" value={nv} onChange={setNv} onCommit={(v) => { setNv(v); set({ notificationVolume: v / 100 }); playSound("preview", s.notification, v / 100); }} />
          <span className="small mono value">{nv}</span>
        </Field>
        <CustomSound kind="notification" value={s.notification} volume={nv / 100} />
      </Group>

      <Group title="Таймер">
        <Field label="Звук по окончании таймера">
          <Toggle checked={s.timerEnabled} onChange={(timerEnabled) => set({ timerEnabled })} />
        </Field>
        <Field label="Сигнал" wide>
          <SoundGrid list={TIMER_SOUNDS} value={s.timer} volume={tv / 100} loop onPick={(timer) => { set({ timer }); playSound("preview", timer, tv / 100, { loop: true, limitSecs: 3 }); }} />
        </Field>
        <Field label="Громкость">
          <Slider label="Громкость таймера" value={tv} onChange={setTv} onCommit={(v) => { setTv(v); set({ timerVolume: v / 100 }); playSound("preview", s.timer, v / 100, { loop: true, limitSecs: 2.5 }); }} />
          <span className="small mono value">{tv}</span>
        </Field>
        <Field label="Сколько звучит" description="Остановить можно кнопкой «Закрыть» или нажатием на остров">
          <Segmented value={String(s.timerLimitSecs)} options={LIMITS} onChange={(v) => set({ timerLimitSecs: Number(v) })} />
        </Field>
        <CustomSound kind="timer" value={s.timer} volume={tv / 100} />
      </Group>
    </Page>
  );
}

// ---------------------------------------------------------------- Шрифты


function FontPreview({ id }: { id: string }) {
  const [family, setFamily] = useState<string | null>(null);
  useEffect(() => {
    let alive = true;
    resolveFont(id).then((f) => alive && setFamily(f));
    return () => {
      alive = false;
    };
  }, [id]);
  return (
    <div className="font-preview" style={family ? { fontFamily: family, fontSizeAdjust: id === "system" ? "none" : 0.5 } : undefined}>
      <div className="fp-pill">
        <span className="fp-time">12:45</span>
        <span className="fp-date">{new Date().toLocaleDateString(document.documentElement.lang === "en" ? "en-US" : "ru-RU", { weekday: "short", day: "numeric", month: "short" })}</span>
      </div>
      <div className="fp-title">Новое сообщение</div>
      <div className="fp-body">Привет! Как тебе новый шрифт острова? Съешь же ещё этих мягких французских булок.</div>
      <div className="fp-latin">The quick brown fox jumps over the lazy dog 0123456789</div>
    </div>
  );
}

export function FontsSection() {
  const current = useApp((x) => x.settings!.appearance.font ?? "system");
  const update = useApp((x) => x.update);
  const [selected, setSelected] = useState(current);
  const custom = useLoad(() => ipc.fontsList());
  const { error, run, busy, setError } = useAction();
  useEffect(() => setSelected(current), [current]);

  // Свои шрифты подгружаем, чтобы карточки показывали их начертание.
  const [customFamilies, setCustomFamilies] = useState<Record<string, string>>({});
  useEffect(() => {
    (custom.data ?? []).forEach((f) =>
      loadCustomFont(f.file)
        .then((fam) => setCustomFamilies((m) => ({ ...m, [f.file]: fam })))
        .catch(() => {}),
    );
  }, [custom.data]);

  const apply = (id: string) => run(() => update("appearance", { font: id }));
  const upload = () =>
    run(async () => {
      const f = await ipc.fontPick();
      if (!f) return;
      if (!(await verifyCustomFont(f.file))) {
        await ipc.fontRemove(f.file).catch(() => {});
        throw new Error("Шрифт не удалось отобразить — файл повреждён или не содержит нужных символов");
      }
      await custom.reload();
      setSelected(`custom:${f.file}`);
    });

  const all = useMemo(
    () => [...FONTS.map((f) => ({ id: f.id, label: f.label, family: f.family })), ...(custom.data ?? []).map((f) => ({ id: `custom:${f.file}`, label: f.family, family: customFamilies[f.file] ?? "inherit" }))],
    [custom.data, customFamilies],
  );
  const changed = selected !== current;
  return (
    <Page title="Шрифты">
      <Group>
        <div className="font-grid">
          {all.map((f) => (
            <button key={f.id} type="button" className={`font-card ${selected === f.id ? "sel" : ""} ${current === f.id ? "cur" : ""}`} onClick={() => setSelected(f.id)} style={{ fontFamily: f.family }}>
              <span className="fc-aa">Аа</span>
              <span className="fc-name">{f.label}</span>
              {current === f.id && <Check size={13} className="fc-check" />}
            </button>
          ))}
        </div>
      </Group>
      <Group title="Предпросмотр">
        <FontPreview id={selected} />
        <div className="row gap font-actions">
          <Button variant="primary" disabled={!changed || busy} onClick={() => apply(selected)}>
            Применить
          </Button>
          <Button disabled={current === "system" || busy} onClick={() => apply("system")}>
            <RotateCcw size={14} /> Стандартный
          </Button>
          <span className="grow" />
          <Button disabled={busy} onClick={upload}>
            {busy ? <Spinner size={14} /> : <Upload size={14} />} Загрузить TTF или OTF
          </Button>
        </div>
      </Group>
      {(custom.data?.length ?? 0) > 0 && (
        <Group title="Свои шрифты">
          {custom.data!.map((f) => (
            <Field key={f.file} label={f.family} description={`${f.file} · ${(f.size / 1024).toFixed(0)} КБ`}>
              <IconButton
                size="sm"
                title="Удалить шрифт"
                onClick={() =>
                  run(async () => {
                    await ipc.fontRemove(f.file);
                    await custom.reload();
                    if (selected === `custom:${f.file}`) setSelected("system");
                  })
                }
              >
                <Trash2 size={14} />
              </IconButton>
            </Field>
          ))}
        </Group>
      )}
      {error && (
        <Note kind="error">
          {error}{" "}
          <button type="button" className="link" onClick={() => setError(null)}>
            Скрыть
          </button>
        </Note>
      )}
    </Page>
  );
}

// ---------------------------------------------------------------- VPN и индикаторы

export function VpnSection() {
  const v = useApp((x) => x.settings!.vpn);
  const update = useApp((x) => x.update);
  const status = useLoad(() => ipc.vpnStatus(), [v.client, v.path]);
  const { error, run } = useAction();
  const st = status.data;
  return (
    <Page title="VPN">
      <Group>
        <Field label="Программа" description="Кнопка VPN в центре управления">
          <Segmented
            value={v.client}
            onChange={(client) => update("vpn", { client }).catch(() => {})}
            options={[
              { value: "auto", label: "Авто" },
              { value: "happ", label: "Happ" },
              { value: "incy", label: "INCY" },
              { value: "none", label: "Нет" },
            ]}
          />
        </Field>
        {v.client !== "none" && (
          <>
            <Field
              label={!st ? "Проверяю…" : !st.client ? "Клиент не найден" : `${st.name}: ${st.connected ? "подключён" : "отключён"}`}
              description={
                !st?.client
                  ? "Установите Happ или INCY или укажите путь к программе"
                  : st.canControl
                    ? "Кнопка подключает сервер, выбранный в самом приложении. Выберите его там один раз."
                    : `${st.name} не умеет подключаться по команде — кнопка открывает приложение`
              }
            >
              <Button small onClick={() => status.reload()}>
                Обновить
              </Button>
            </Field>
            <Field label="Путь к программе" description={v.path || st?.path || "Определяется автоматически"}>
              <div className="row gap">
                {v.path && (
                  <Button small onClick={() => update("vpn", { path: "" }).catch(() => {})}>
                    Сбросить
                  </Button>
                )}
                <Button small onClick={() => run(() => ipc.vpnPickPath())}>
                  <FolderOpen size={14} /> Выбрать
                </Button>
              </div>
            </Field>
            <Field label="Показывать подключение в острове">
              <Toggle checked={v.notify} onChange={(notify) => update("vpn", { notify }).catch(() => {})} />
            </Field>
          </>
        )}
      </Group>
      {error && <Note kind="error">{errorMessage(error)}</Note>}
    </Page>
  );
}

export function IndicatorsSection() {
  const k = useApp((x) => x.settings!.keyboard);
  const steam = useApp((x) => x.settings!.modules.steam);
  const update = useApp((x) => x.update);
  return (
    <Page title="Индикаторы">
      <Group>
        <Field label="Смена раскладки" description="Остров коротко показывает RU, EN и другие">
          <Toggle checked={k.layout} onChange={(layout) => update("keyboard", { layout }).catch(() => {})} />
        </Field>
        <Field label="Caps Lock" description="Вспышка при переключении и значок, пока включён">
          <Toggle checked={k.caps} onChange={(caps) => update("keyboard", { caps }).catch(() => {})} />
        </Field>
        <Field label="Загрузки Steam" description="Игра, прогресс, скорость и сколько осталось">
          <Toggle checked={steam} onChange={(v) => update("modules", { steam: v }).catch(() => {})} />
        </Field>
      </Group>
    </Page>
  );
}

// ---------------------------------------------------------------- Об авторе

function Typed({ text }: { text: string }) {
  const [n, setN] = useState(0);
  useEffect(() => {
    setN(0);
    const t = window.setInterval(() => setN((x) => (x >= text.length ? x : x + 1)), 38);
    return () => window.clearInterval(t);
  }, [text]);
  return (
    <span className="typed">
      {text.slice(0, n)}
      <i className="caret" aria-hidden />
    </span>
  );
}

export function AuthorSection() {
  const version = useLoad(() => ipc.about());
  const lang = useApp((x) => x.settings!.appearance.language);
  return (
    <div className="bio-page">
      <div className="bio-bg" style={{ backgroundImage: `url(${AUTHOR.avatar})` }} aria-hidden />
      <div className="bio-card">
        <div className="bio-banner">
          <span className="bio-views">v{version.data?.version ?? "…"}</span>
        </div>
        <img className="bio-avatar" src={AUTHOR.avatar} alt="" draggable={false} />
        <h1 className="bio-name">{AUTHOR.name}</h1>
        <div className="bio-handle">@{AUTHOR.handle}</div>
        <div className="bio-badges">
          {AUTHOR.badges.map((b) => (
            <span key={b.label} className="bio-badge" title={b.label}>
              <b.icon size={14} />
            </span>
          ))}
        </div>
        <p className="bio-text">
          <Typed text={lang === "en" ? AUTHOR.bioEn : AUTHOR.bio} />
        </p>
        <div className="bio-divider" />
        <div className="bio-links">
          {AUTHOR.links.map((l, i) => (
            <button key={l.url} type="button" className="bio-link" style={{ animationDelay: `${120 + i * 60}ms` }} onClick={() => ipc.openUrl(l.url).catch(() => {})}>
              <span className="bio-link-icon">
                <l.icon size={18} />
              </span>
              <span className="bio-link-label">{l.label}</span>
            </button>
          ))}
        </div>
        {AUTHOR.contact && <div className="bio-contact">{AUTHOR.contact}</div>}
      </div>
    </div>
  );
}
