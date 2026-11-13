// Голос → текст на компьютере (Vosk): загрузка модели и настройки.

import { useState } from "react";
import { Button, Note, Segmented, Toggle } from "../../components/ui";
import { useAction, useEvent, useLoad } from "../../hooks/common";
import { ipc } from "../../services/ipc";
import { useApp } from "../../stores/app";
import { Field, Group, Page } from "../components";

export function SpeechSection() {
  const s = useApp((x) => x.settings!.speech);
  const update = useApp((x) => x.update);
  const status = useLoad(() => ipc.speechStatus());
  const { error, run, busy } = useAction();
  const [progress, setProgress] = useState<{ what: string; done: number; total: number } | null>(null);
  useEvent<{ what: string; done: number; total: number }>("speech://progress", (p) => {
    setProgress(p.what === "done" ? null : p);
    if (p.what === "done") status.reload();
  });
  const ready = status.data?.models.includes(s.lang);
  const pct = progress && progress.total ? Math.round((progress.done / progress.total) * 100) : null;
  return (
    <Page title="Распознавание речи">
      <Group>
        <Field label="Язык">
          <Segmented
            value={s.lang}
            onChange={(lang) => update("speech", { lang }).catch(() => {})}
            options={[
              { value: "ru", label: "Русский" },
              { value: "en", label: "English" },
            ]}
          />
        </Field>
        <Field
          label={ready ? "Модель скачана" : "Модель не скачана"}
          description={ready ? "Голосовые заметки превращаются в текст прямо на компьютере, без интернета" : "Один раз скачается библиотека Vosk и модель языка (около 45 МБ)"}
        >
          {!ready && (
            <Button variant="primary" disabled={busy || !!progress} onClick={() => run(() => ipc.speechInstall(s.lang).then(() => status.reload()))}>
              {progress ? (pct != null ? `${progress.what === "model" ? "Модель" : "Библиотека"}: ${pct}%` : "Загрузка…") : "Скачать"}
            </Button>
          )}
        </Field>
        <Field label="Сразу превращать голосовые заметки в текст">
          <Toggle checked={s.auto} onChange={(auto) => update("speech", { auto }).catch(() => {})} />
        </Field>
      </Group>
      {error && <Note kind="error">{error}</Note>}
    </Page>
  );
}
