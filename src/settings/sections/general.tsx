// Разделы: Общие, Внешний вид, Положение панели.

import { useState } from "react";
import { Button, Note, Segmented, Slider, Spinner, Toggle } from "../../components/ui";
import { useAction } from "../../hooks/common";
import { ipc } from "../../services/ipc";
import { useApp } from "../../stores/app";
import type { AnimationLevel, UpdateInfo } from "../../types";
import { Field, Group, Page } from "../components";

export function GeneralSection() {
  const s = useApp((x) => x.settings)!;
  const db = useApp((x) => x.db);
  const update = useApp((x) => x.update);
  const { error, run } = useAction();
  return (
    <Page title="Общие" description="Запуск, фоновая работа и поведение панели.">
      {db?.warning && <Note kind="warn">{db.warning}</Note>}
      <Group title="Запуск">
        <Field label="Запускать вместе с системой" description="Desktop Island будет стартовать при входе в систему (свёрнутым в трей, если включено ниже).">
          <Toggle checked={s.general.launchAtStartup} onChange={(v) => run(() => update("general", { launchAtStartup: v }))} />
        </Field>
        <Field label="Работа в фоне" description="Закрытие окон не завершает приложение. Полный выход — через меню трея.">
          <Toggle checked={s.general.backgroundMode} onChange={(v) => run(() => update("general", { backgroundMode: v }))} />
        </Field>
      </Group>
      <Group title="Панель">
        <Field label="Показать / скрыть панель">
          <Button onClick={() => run(() => ipc.islandToggle())}>Переключить</Button>
        </Field>
        <Field label="Скрыть временно" description="Панель вернётся автоматически.">
          <div className="row gap">
            <Button onClick={() => run(() => ipc.islandHideTemporarily(15))}>15 минут</Button>
            <Button onClick={() => run(() => ipc.islandHideTemporarily(60))}>1 час</Button>
          </div>
        </Field>
        <Field label="Завершить Desktop Island" description="Останавливает все модули и закрывает приложение.">
          <Button variant="danger" onClick={() => run(() => ipc.quit())}>Выйти</Button>
        </Field>
      </Group>
      <UpdatesGroup />
      {error && <Note kind="error">{error}</Note>}
    </Page>
  );
}

export function AppearanceSection() {
  const a = useApp((x) => x.settings!.appearance);
  const update = useApp((x) => x.update);
  const set = (p: Partial<typeof a>) => update("appearance", p).catch(() => {});
  return (
    <Page title="Внешний вид">
      <Group title="Язык / Language">
        <Field label="Язык интерфейса">
          <Segmented
            value={a.language ?? "ru"}
            onChange={(language) => set({ language })}
            options={[
              { value: "ru", label: "Русский" },
              { value: "en", label: "English" },
            ]}
          />
        </Field>
      </Group>
      <Group title="Остров">
        <Field label="Цвет острова">
          <Segmented
            value={a.islandTone ?? "graphite"}
            onChange={(islandTone) => set({ islandTone })}
            options={[
              { value: "graphite", label: "Графит" },
              { value: "grey", label: "Серый" },
              { value: "black", label: "Чёрный" },
            ]}
          />
        </Field>
        <Field label="Полоска в полноэкранном режиме" description="Во время полноэкранного видео остров сжимается в тонкую полоску — наведите на неё, чтобы вернуть">
          <Toggle checked={a.fullscreenStrip ?? true} label="Полоска" onChange={(fullscreenStrip) => set({ fullscreenStrip })} />
        </Field>
        <Field label="Приветствие при включении" description="«Доброе утро», «Добрый вечер» — при запуске, пробуждении и разблокировке">
          <Toggle checked={a.greeting ?? true} label="Приветствие" onChange={(greeting) => set({ greeting })} />
        </Field>
      </Group>
      <Group title="Окно настроек">
        <Field label={`Непрозрачность: ${Math.round((a.settingsOpacity ?? 1) * 100)}%`}>
          <Slider value={Math.round((a.settingsOpacity ?? 1) * 100)} min={40} max={100} onCommit={(v) => set({ settingsOpacity: v / 100 })} />
        </Field>
        <Field label="Эффект под окном" description="Только Windows 11 и только при неполной непрозрачности">
          <Segmented
            value={a.settingsEffect ?? "none"}
            onChange={(settingsEffect) => set({ settingsEffect })}
            options={[
              { value: "none", label: "Нет" },
              { value: "mica", label: "Mica" },
              { value: "acrylic", label: "Акрил" },
            ]}
          />
        </Field>
        <Field label="Обои" description={a.wallpaper ? "Своя картинка выбрана" : "Своя картинка на фоне окна настроек"}>
          <span className="row gap">
            <Button onClick={() => ipc.wallpaperPick().catch(() => {})}>Выбрать</Button>
            {a.wallpaper && <Button onClick={() => ipc.wallpaperClear().catch(() => {})}>Убрать</Button>}
          </span>
        </Field>
        {a.wallpaper && (
          <Field label={`Затемнение обоев: ${Math.round((a.wallpaperDim ?? 0.45) * 100)}%`}>
            <Slider value={Math.round((a.wallpaperDim ?? 0.45) * 100)} min={0} max={90} onCommit={(v) => set({ wallpaperDim: v / 100 })} />
          </Field>
        )}
        {a.wallpaper && (
          <Field label={`Размытие обоев: ${a.wallpaperBlur ?? 12} px`}>
            <Slider value={a.wallpaperBlur ?? 12} min={0} max={40} onCommit={(v) => set({ wallpaperBlur: v })} />
          </Field>
        )}
      </Group>
      <Group title="Тема">
        <Field label="Режим">
          <Segmented
            value={a.mode}
            onChange={(mode) => set({ mode })}
            options={[
              { value: "system", label: "Системная" },
              { value: "dark", label: "Тёмная" },
              { value: "light", label: "Светлая" },
            ]}
          />
        </Field>
        <Field label="Цветовая схема" description="Выбор и импорт тем — в разделе «Темы».">
          <Button onClick={() => ipc.openSettings("themes")}>Открыть темы</Button>
        </Field>
      </Group>
      <Group title="Панель">
        <Field label={`Непрозрачность: ${Math.round(a.opacity * 100)}%`}>
          <Slider value={Math.round(a.opacity * 100)} min={35} max={100} onCommit={(v) => set({ opacity: v / 100 })} />
        </Field>
        <Field label={`Скругление раскрытой панели: ${a.radius}px`}>
          <Slider value={a.radius} min={6} max={40} onCommit={(v) => set({ radius: v })} />
        </Field>
        <Field label={`Масштаб: ${Math.round(a.scale * 100)}%`}>
          <Slider value={Math.round(a.scale * 100)} min={75} max={150} step={5} onCommit={(v) => set({ scale: v / 100 })} />
        </Field>
      </Group>
      <Group title="Анимации">
        <Field label="Уровень анимаций" description="«Упрощённые» и «Выключены» снижают нагрузку на GPU.">
          <Segmented<AnimationLevel>
            value={a.animations}
            onChange={(animations) => set({ animations })}
            options={[
              { value: "full", label: "Полные" },
              { value: "reduced", label: "Упрощённые" },
              { value: "off", label: "Выключены" },
            ]}
          />
        </Field>
        <Field label={`Скорость анимаций: ×${a.animationSpeed.toFixed(2)}`}>
          <Slider value={a.animationSpeed * 100} min={25} max={300} step={5} onCommit={(v) => set({ animationSpeed: v / 100 })} />
        </Field>
      </Group>
    </Page>
  );
}

export function PositionSection() {
  const p = useApp((x) => x.settings!.position);
  const update = useApp((x) => x.update);
  const { error, run } = useAction();
  return (
    <Page title="Положение">
      <div className="pos-picker">
        {(["edge", "float"] as const).map((style) => (
          <button
            key={style}
            type="button"
            className={`pos-card ${p.style === style ? "on" : ""}`}
            aria-pressed={p.style === style}
            onClick={() => run(() => update("position", { style }).then(() => ipc.islandCenter()))}
          >
            <span className="pos-desk">
              <span className={`pos-isle ${style}`} />
            </span>
            <span className="pos-label">
              <span className="pos-dot" />
              {style === "edge" ? "У края экрана" : "Плавающий"}
            </span>
          </button>
        ))}
      </div>
      <Group>
        <Field label="Закрепить положение" description="Запрещает перетаскивание панели.">
          <Toggle checked={p.locked} onChange={(v) => run(() => update("position", { locked: v }))} />
        </Field>
        <Field label="Поставить по центру" description="Возвращает панель к верхнему краю по центру текущего монитора. Та же кнопка есть в самой панели (значок прицела) и в меню трея.">
          <Button variant="primary" onClick={() => run(() => ipc.islandCenter())}>По центру</Button>
        </Field>
      </Group>
      <Group title="Сохранённые положения">
        {Object.keys(p.placements).length === 0 && <div className="muted small">Используется положение по умолчанию.</div>}
        {Object.entries(p.placements).map(([k, v]) => (
          <Field key={k} label={k.replace(/^\\\\\.\\/, "")}description={`Центр: ${Math.round(v.relX * 100)}% ширины, отступ сверху ${Math.round(v.top)} px`} />
        ))}
        <div className="small muted">Положение хранится относительно каждого монитора, поэтому корректно переживает смену разрешения и переподключение дисплея.</div>
      </Group>
      {error && <Note kind="error">{error}</Note>}
    </Page>
  );
}

function UpdatesGroup() {
  const s = useApp((x) => x.settings!.general);
  const update = useApp((x) => x.update);
  const [info, setInfo] = useState<UpdateInfo | null>(null);
  const { error, busy, run } = useAction();
  const check = () => run(async () => setInfo(await ipc.updateCheck()));
  return (
    <Group title="Обновления">
      <Field label="Проверять обновления автоматически" description="Раз в 6 часов Desktop Island спрашивает у GitHub, вышла ли новая версия. Никакие ваши данные не отправляются.">
        <Toggle checked={s.checkUpdates} onChange={(v) => run(() => update("general", { checkUpdates: v }))} />
      </Field>
      <Field
        label="Проверить сейчас"
        description={
          info
            ? info.available
              ? `Доступна версия ${info.latest} (у вас ${info.current})`
              : `У вас последняя версия ${info.current}`
            : undefined
        }
      >
        <div className="row gap">
          <Button onClick={check} disabled={busy}>{busy ? <Spinner size={14} /> : "Проверить"}</Button>
          {info?.available && (
            <Button variant="primary" disabled={busy} onClick={() => run(() => ipc.updateInstall())}>
              {info.method === "page" ? "Открыть страницу загрузки" : "Обновить"}
            </Button>
          )}
        </div>
      </Field>
      {error && <Note kind="error">{error}</Note>}
    </Group>
  );
}
