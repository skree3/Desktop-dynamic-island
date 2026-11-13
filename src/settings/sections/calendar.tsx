// Календарь по ICS-ссылкам и настройки помодоро.

import { RefreshCw } from "lucide-react";
import { Button, Note, Toggle } from "../../components/ui";
import { useEvent, useLoad } from "../../hooks/common";
import { ipc } from "../../services/ipc";
import { useApp } from "../../stores/app";
import type { CalEvent } from "../../types";
import { Field, Group, ListEditor, NumberInput, Page } from "../components";

function when(e: CalEvent): string {
  const d = new Date(e.start);
  const today = new Date();
  const tomorrow = new Date(Date.now() + 86_400_000);
  const day = d.toDateString() === today.toDateString() ? "сегодня" : d.toDateString() === tomorrow.toDateString() ? "завтра" : d.toLocaleDateString("ru-RU", { day: "numeric", month: "short" });
  return e.allDay ? day : `${day}, ${d.toLocaleTimeString("ru-RU", { hour: "2-digit", minute: "2-digit" })}`;
}

export function CalendarSection() {
  const c = useApp((x) => x.settings!.calendar);
  const update = useApp((x) => x.update);
  const events = useLoad(() => ipc.calendarEvents());
  useEvent("calendar://changed", () => events.reload());
  const [list, error] = events.data ?? [[], null];
  return (
    <Page title="Календарь">
      <Group>
        <Field
          wide
          label="Ссылки на календари"
          description="Секретная ссылка в формате iCal (ICS): Google Календарь → Настройки календаря → «Закрытый адрес в формате iCal»; Outlook → «Опубликовать календарь»; Яндекс → «Экспорт»"
        >
          <ListEditor items={c.urls} onChange={(urls) => update("calendar", { urls }).catch(() => {})} placeholder="https://… .ics" />
        </Field>
        <Field label="Напоминать заранее">
          <NumberInput value={c.remindMinutes} min={0} max={120} suffix="мин" onChange={(remindMinutes) => update("calendar", { remindMinutes }).catch(() => {})} />
        </Field>
      </Group>
      {error && <Note kind="warn">Календарь не загрузился: {error}</Note>}
      {c.urls.length > 0 && (
        <Group title="Ближайшие события">
          {list.length === 0 ? (
            <div className="field">
              <div className="field-text">
                <div className="field-desc">Событий нет или календарь ещё загружается</div>
              </div>
            </div>
          ) : (
            list.slice(0, 8).map((e) => (
              <Field key={e.uid + e.start} label={e.title || "Без названия"} description={[when(e), e.location].filter(Boolean).join(" · ")} />
            ))
          )}
          <div className="field">
            <div className="field-text" />
            <Button onClick={() => ipc.calendarRefresh().then(() => setTimeout(events.reload, 2500))}>
              <RefreshCw size={14} /> Обновить
            </Button>
          </div>
        </Group>
      )}
    </Page>
  );
}

export function PomodoroSection() {
  const p = useApp((x) => x.settings!.pomodoro);
  const update = useApp((x) => x.update);
  const set = (patch: Partial<typeof p>) => update("pomodoro", patch).catch(() => {});
  return (
    <Page title="Помодоро">
      <Group>
        <Field label="Фокус">
          <NumberInput value={p.focusMin} min={1} max={180} suffix="мин" onChange={(focusMin) => set({ focusMin })} />
        </Field>
        <Field label="Короткий перерыв">
          <NumberInput value={p.shortMin} min={1} max={60} suffix="мин" onChange={(shortMin) => set({ shortMin })} />
        </Field>
        <Field label="Длинный перерыв">
          <NumberInput value={p.longMin} min={1} max={120} suffix="мин" onChange={(longMin) => set({ longMin })} />
        </Field>
        <Field label="Фокусов до длинного перерыва">
          <NumberInput value={p.cycles} min={1} max={12} onChange={(cycles) => set({ cycles })} />
        </Field>
        <Field label="«Не беспокоить» во время фокуса" description="Уведомления копятся в истории и не отвлекают">
          <Toggle checked={p.dndDuringFocus} onChange={(dndDuringFocus) => set({ dndDuringFocus })} />
        </Field>
      </Group>
    </Page>
  );
}
