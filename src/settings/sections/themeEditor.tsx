// Редактор своей темы: цвета (из 1–3 основных или все вручную, отдельно для
// тёмной и светлой палитры), остров, скругления, градиентный фон и прозрачность
// карточек. Изменения можно сразу посмотреть на настоящем острове.

import { useEffect, useMemo, useRef, useState } from "react";
import { Eye, EyeOff, ImageUp, RotateCcw } from "lucide-react";
import { Button, Note, Segmented, Slider, Toggle } from "../../components/ui";
import { useAction } from "../../hooks/common";
import { useResolvedMode } from "../../hooks/useTheme";
import { ipc } from "../../services/ipc";
import { useApp } from "../../stores/app";
import type { ThemeRow } from "../../types";
import { alphaOf, buildPalettes, paletteFromImage, toHex6, withAlpha, type Palette } from "../../utils/style";
import { Group } from "../components";

const DEF = { accent: "#f0a64a", base: "#2b4a6f", tint: "#c9a0ff" };
const isHex = (c: unknown): c is string => typeof c === "string" && /^#[0-9a-fA-F]{6}$/.test(c.trim());

/** Цвета палитры в том порядке, в каком их показывает ручной режим. */
const KEYS: { key: keyof Palette; label: string }[] = [
  { key: "bg", label: "Фон" },
  { key: "surface", label: "Карточки" },
  { key: "surfaceHover", label: "Карточки при наведении" },
  { key: "border", label: "Обводки" },
  { key: "text", label: "Текст" },
  { key: "textMuted", label: "Второстепенный текст" },
  { key: "accent", label: "Акцент" },
  { key: "accentText", label: "Текст на акценте" },
  { key: "success", label: "Успех" },
  { key: "warning", label: "Предупреждение" },
  { key: "danger", label: "Ошибка" },
];

type Tab = "colors" | "island" | "shape" | "background";

interface Draft {
  name: string;
  accent: string;
  base: string;
  useBase: boolean;
  tint: string;
  useTint: boolean;
  manual: boolean;
  palettes: { dark: Palette; light: Palette };
  island: { on: boolean; color: string; border: string; useBorder: boolean; glow: boolean };
  shape: { on: boolean; island: number; card: number; button: number };
  bg: { on: boolean; colors: string[]; three: boolean; angle: number };
  cards: { on: boolean; opacity: number; blur: number };
}

function initDraft(t: ThemeRow | null): Draft {
  const src = t?.data.source;
  const accent = isHex(src?.accent) ? src!.accent : isHex(t?.data.dark?.accent) ? t!.data.dark.accent : DEF.accent;
  const base = isHex(src?.base) ? src!.base! : DEF.base;
  const useBase = t ? isHex(src?.base) : true;
  const tint = isHex(src?.tint) ? src!.tint! : DEF.tint;
  const useTint = isHex(src?.tint);
  const auto = buildPalettes(accent, useBase ? base : null, useTint ? tint : null);
  const manual = !!t?.data.manual;
  const fromTheme = (m: "dark" | "light"): Palette => ({ ...auto[m], ...(t?.data[m] as Partial<Palette>) });
  const isl = t?.data.island;
  const sh = t?.data.shape;
  const bg = t?.data.background;
  const cd = t?.data.cards;
  return {
    name: t?.name ?? "Мой стиль",
    accent,
    base,
    useBase,
    tint,
    useTint,
    manual,
    palettes: manual ? { dark: fromTheme("dark"), light: fromTheme("light") } : auto,
    island: { on: !!isl, color: isl?.color ?? "#1c1c1f", border: isl?.border ?? "#ffffff", useBorder: !!isl?.border, glow: !!isl?.glow },
    shape: { on: !!sh, island: sh?.island ?? 30, card: sh?.card ?? 14, button: sh?.button ?? 8 },
    bg: { on: !!bg, colors: [bg?.colors[0] ?? "#1b2a4a", bg?.colors[1] ?? "#4a1b3d", bg?.colors[2] ?? "#1b4a3a"], three: (bg?.colors.length ?? 2) === 3, angle: bg?.angle ?? 135 },
    cards: { on: !!cd, opacity: cd?.opacity ?? 0.7, blur: cd?.blur ?? 12 },
  };
}

/** Тема в формате бэкенда (тот же, что для кода и файла). */
function toTheme(d: Draft, pal: { dark: Palette; light: Palette }, id?: string) {
  return {
    id,
    name: d.name.trim() || "Мой стиль",
    dark: pal.dark,
    light: pal.light,
    source: { accent: d.accent, base: d.useBase ? d.base : null, tint: d.useTint ? d.tint : null },
    manual: d.manual || undefined,
    island: d.island.on ? { color: d.island.color, border: d.island.useBorder ? d.island.border : null, glow: d.island.glow } : undefined,
    shape: d.shape.on ? { island: d.shape.island, card: d.shape.card, button: d.shape.button } : undefined,
    background: d.bg.on ? { colors: d.bg.three ? d.bg.colors : d.bg.colors.slice(0, 2), angle: d.bg.angle } : undefined,
    cards: d.cards.on ? { opacity: d.cards.opacity, blur: d.cards.blur } : undefined,
  };
}

function ColorRow({ label, value, onChange, disabled, children }: { label: string; value: string; onChange: (v: string) => void; disabled?: boolean; children?: React.ReactNode }) {
  return (
    <label className={`color-pick ${disabled ? "off" : ""}`}>
      <input type="color" value={toHex6(value)} disabled={disabled} onChange={(e) => onChange(e.target.value)} />
      <span className="grow">{label}</span>
      {children}
    </label>
  );
}

export function StyleEditor({ editing, onSaved, onCancel }: { editing: ThemeRow | null; onSaved: (id: string, edited: boolean) => void; onCancel: () => void }) {
  const [d, setD] = useState<Draft>(() => initDraft(editing));
  const [tab, setTab] = useState<Tab>("colors");
  const mode = useResolvedMode();
  const [palMode, setPalMode] = useState<"dark" | "light">(mode);
  const [live, setLive] = useState(false);
  const fromImage = useApp((s) => s.settings?.appearance.themeFromImage ?? false);
  const { error, run, busy } = useAction();
  const ref = useRef<HTMLDivElement>(null);
  const fileRef = useRef<HTMLInputElement>(null);
  const patch = <K extends keyof Draft>(k: K, v: Partial<Draft[K]>) => setD((x) => ({ ...x, [k]: typeof v === "object" && !Array.isArray(v) ? { ...(x[k] as object), ...v } : v }));

  useEffect(() => {
    if (editing) ref.current?.scrollIntoView({ behavior: "smooth", block: "center" });
  }, [editing]);

  const pal = useMemo(() => (d.manual ? d.palettes : buildPalettes(d.accent, d.useBase ? d.base : null, d.useTint ? d.tint : null)), [d]);
  const p = pal[mode];
  const json = useMemo(() => JSON.stringify(toTheme(d, pal)), [d, pal]);

  // Предпросмотр на острове: обновляется вслед за правками, снимается при закрытии редактора.
  const sentExpand = useRef(false);
  useEffect(() => {
    if (!live) return;
    const t = window.setTimeout(() => {
      ipc.themePreview(json, !sentExpand.current).catch(() => {});
      sentExpand.current = true;
    }, 150);
    return () => window.clearTimeout(t);
  }, [live, json]);
  useEffect(() => {
    if (!live) {
      sentExpand.current = false;
      ipc.themePreview(null).catch(() => {});
    }
  }, [live]);
  useEffect(() => () => void ipc.themePreview(null).catch(() => {}), []);

  const setManual = (on: boolean) => setD((x) => ({ ...x, manual: on, palettes: on ? buildPalettes(x.accent, x.useBase ? x.base : null, x.useTint ? x.tint : null) : x.palettes }));
  const setPalColor = (key: keyof Palette, hex6: string) =>
    setD((x) => {
      const cur = x.palettes[palMode][key];
      const next = withAlpha(hex6, alphaOf(cur));
      return { ...x, palettes: { ...x.palettes, [palMode]: { ...x.palettes[palMode], [key]: next } } };
    });

  const pickImage = (f: File | undefined) =>
    f &&
    run(async () => {
      const c = await paletteFromImage(f);
      setD((x) => ({ ...x, manual: false, accent: c.accent, base: c.base, useBase: true, tint: c.tint ?? x.tint, useTint: !!c.tint }));
    });

  const save = () =>
    run(async () => {
      const id = editing?.id ?? `custom-${Date.now().toString(36)}`;
      await ipc.themeSave(JSON.stringify(toTheme(d, pal, id)));
      useApp.setState({ themes: await ipc.themes() });
      setLive(false);
      onSaved(id, !!editing);
    });

  const bgImage = d.bg.on ? `linear-gradient(${d.bg.angle}deg, ${(d.bg.three ? d.bg.colors : d.bg.colors.slice(0, 2)).join(", ")})` : undefined;
  const cardBg = (c: string) => (d.cards.on ? withAlpha(toHex6(c), d.cards.opacity * alphaOf(c)) : c);
  const rCard = d.shape.on ? d.shape.card : 10;
  const islandColor = d.island.on ? d.island.color : "#1c1c1f";

  return (
    <div ref={ref}>
      <Group title={editing ? `Изменение: ${editing.name}` : "Свой стиль"}>
        <div className="field wide">
          <div className="style-editor">
            <div className="style-colors">
              <Segmented
                value={tab}
                onChange={setTab}
                options={[
                  { value: "colors", label: "Цвета" },
                  { value: "island", label: "Остров" },
                  { value: "shape", label: "Форма" },
                  { value: "background", label: "Фон и карточки" },
                ]}
              />
              <div className="style-tab" key={tab}>
                {tab === "colors" && (
                  <>
                    <ColorRow label="Акцент" value={d.accent} disabled={d.manual} onChange={(v) => patch("accent", v as never)} />
                    <ColorRow label="Оттенок фона" value={d.base} disabled={d.manual || !d.useBase} onChange={(v) => patch("base", v as never)}>
                      <Toggle checked={d.useBase} disabled={d.manual} label="Оттенок фона" onChange={(v) => patch("useBase", v as never)} />
                    </ColorRow>
                    <ColorRow label="Оттенок текста" value={d.tint} disabled={d.manual || !d.useTint} onChange={(v) => patch("tint", v as never)}>
                      <Toggle checked={d.useTint} disabled={d.manual} label="Оттенок текста" onChange={(v) => patch("useTint", v as never)} />
                    </ColorRow>
                    {fromImage && (
                      <>
                        <input ref={fileRef} type="file" accept="image/*" hidden onChange={(e) => pickImage(e.target.files?.[0])} />
                        <Button small disabled={busy} onClick={() => fileRef.current?.click()}>
                          <ImageUp size={14} /> Подобрать из картинки…
                        </Button>
                      </>
                    )}
                    <label className="color-pick toggle-row">
                      <span className="grow">
                        Все цвета вручную
                        <small>Фон, карточки, текст, обводки и цвета состояний — отдельно для тёмной и светлой темы</small>
                      </span>
                      <Toggle checked={d.manual} label="Все цвета вручную" onChange={setManual} />
                    </label>
                    {d.manual && (
                      <>
                        <div className="row between">
                          <Segmented
                            value={palMode}
                            onChange={setPalMode}
                            options={[
                              { value: "dark", label: "Тёмная" },
                              { value: "light", label: "Светлая" },
                            ]}
                          />
                          <Button small title="Пересчитать из акцента и оттенков" onClick={() => setManual(true)}>
                            <RotateCcw size={13} /> Из основы
                          </Button>
                        </div>
                        <div className="manual-grid">
                          {KEYS.map((k) => (
                            <ColorRow key={k.key} label={k.label} value={d.palettes[palMode][k.key]} onChange={(v) => setPalColor(k.key, v)} />
                          ))}
                        </div>
                      </>
                    )}
                  </>
                )}
                {tab === "island" && (
                  <>
                    <ColorRow label="Свой цвет острова" value={d.island.color} disabled={!d.island.on} onChange={(color) => patch("island", { color })}>
                      <Toggle checked={d.island.on} label="Свой цвет острова" onChange={(on) => patch("island", { on })} />
                    </ColorRow>
                    <ColorRow label="Обводка" value={d.island.border} disabled={!d.island.on || !d.island.useBorder} onChange={(border) => patch("island", { border })}>
                      <Toggle checked={d.island.useBorder} disabled={!d.island.on} label="Обводка" onChange={(useBorder) => patch("island", { useBorder })} />
                    </ColorRow>
                    <label className="color-pick toggle-row">
                      <span className="grow">
                        Свечение
                        <small>Мягкий ореол цветом акцента вокруг острова</small>
                      </span>
                      <Toggle checked={d.island.glow} disabled={!d.island.on} label="Свечение" onChange={(glow) => patch("island", { glow })} />
                    </label>
                    <div className="small muted">Стиль острова (стекло, неон, аура, графит) выбирается в «Внешнем виде».</div>
                  </>
                )}
                {tab === "shape" && (
                  <>
                    <label className="color-pick toggle-row">
                      <span className="grow">Своё скругление углов</span>
                      <Toggle checked={d.shape.on} label="Своё скругление" onChange={(on) => patch("shape", { on })} />
                    </label>
                    {(
                      [
                        ["island", "Остров", 6, 32],
                        ["card", "Карточки", 0, 28],
                        ["button", "Кнопки и поля", 0, 20],
                      ] as const
                    ).map(([k, label, min, max]) => (
                      <div key={k} className="slider-row">
                        <span>
                          {label}: {d.shape[k]} px
                        </span>
                        <Slider value={d.shape[k]} min={min} max={max} disabled={!d.shape.on} label={label} onChange={(v) => patch("shape", { [k]: v })} />
                      </div>
                    ))}
                  </>
                )}
                {tab === "background" && (
                  <>
                    <label className="color-pick toggle-row">
                      <span className="grow">
                        Градиентный фон
                        <small>Фон окна настроек и острова (в обычном стиле)</small>
                      </span>
                      <Toggle checked={d.bg.on} label="Градиентный фон" onChange={(on) => patch("bg", { on })} />
                    </label>
                    <div className="row gap">
                      {d.bg.colors.slice(0, d.bg.three ? 3 : 2).map((c, i) => (
                        <label key={i} className="color-pick">
                          <input type="color" value={c} disabled={!d.bg.on} aria-label={`Цвет ${i + 1}`} onChange={(e) => patch("bg", { colors: d.bg.colors.map((x, j) => (j === i ? e.target.value : x)) })} />
                        </label>
                      ))}
                      <span className="grow" />
                      <span className="small muted">Три цвета</span>
                      <Toggle checked={d.bg.three} disabled={!d.bg.on} label="Три цвета" onChange={(three) => patch("bg", { three })} />
                    </div>
                    <div className="slider-row">
                      <span>Угол: {d.bg.angle}°</span>
                      <Slider value={d.bg.angle} min={0} max={360} disabled={!d.bg.on} label="Угол" onChange={(angle) => patch("bg", { angle })} />
                    </div>
                    <label className="color-pick toggle-row">
                      <span className="grow">
                        Прозрачные карточки
                        <small>Сквозь карточки виден фон, обои или градиент</small>
                      </span>
                      <Toggle checked={d.cards.on} label="Прозрачные карточки" onChange={(on) => patch("cards", { on })} />
                    </label>
                    <div className="slider-row">
                      <span>Непрозрачность: {Math.round(d.cards.opacity * 100)}%</span>
                      <Slider value={Math.round(d.cards.opacity * 100)} min={20} max={100} disabled={!d.cards.on} label="Непрозрачность" onChange={(v) => patch("cards", { opacity: v / 100 })} />
                    </div>
                    <div className="slider-row">
                      <span>Размытие: {d.cards.blur} px</span>
                      <Slider value={d.cards.blur} min={0} max={30} disabled={!d.cards.on} label="Размытие" onChange={(blur) => patch("cards", { blur })} />
                    </div>
                  </>
                )}
              </div>
              <input className="input" style={{ marginTop: 14, width: "100%" }} value={d.name} maxLength={40} aria-label="Название стиля" onChange={(e) => patch("name", e.target.value as never)} />
              <div className="row gap wrap" style={{ marginTop: 8 }}>
                <Button variant="primary" disabled={busy} onClick={save}>
                  {editing ? "Сохранить изменения" : "Сохранить и включить"}
                </Button>
                <Button onClick={() => setLive((v) => !v)} title="Показать тему на настоящем острове, не сохраняя">
                  {live ? <EyeOff size={14} /> : <Eye size={14} />} {live ? "Скрыть с острова" : "Показать на острове"}
                </Button>
                {editing && <Button onClick={onCancel}>Отмена</Button>}
              </div>
              {error && <Note kind="error">{error}</Note>}
            </div>
            <div className="style-preview" style={{ background: p.bg, backgroundImage: bgImage, color: p.text }}>
              <span
                className="pv-island"
                style={{
                  background: islandColor,
                  borderRadius: d.shape.on ? Math.min(12, d.shape.island) : 12,
                  boxShadow: [d.island.on && d.island.useBorder ? `inset 0 0 0 1px ${d.island.border}` : "", d.island.on && d.island.glow ? `0 0 12px ${withAlpha(toHex6(p.accent), 0.5)}` : ""].filter(Boolean).join(", ") || undefined,
                }}
              >
                <i style={{ background: p.accent }} />
                <b style={{ background: "#ededed" }} />
              </span>
              <span className="pv-card" style={{ background: cardBg(p.surface), borderRadius: rCard, backdropFilter: d.cards.on && d.cards.blur ? `blur(${d.cards.blur}px)` : undefined }}>
                Уведомления
                <span className="pv-tg" style={{ background: p.accent }}>
                  <style>{`.pv-tg::after{background:${p.accentText}}`}</style>
                </span>
              </span>
              <span className="pv-card" style={{ background: cardBg(p.surface), borderRadius: rCard, color: p.textMuted }}>
                Музыка
              </span>
              <span className="pv-btn" style={{ background: p.accent, color: p.accentText, borderRadius: d.shape.on ? d.shape.button : 8 }}>
                Кнопка
              </span>
            </div>
          </div>
        </div>
      </Group>
    </div>
  );
}
