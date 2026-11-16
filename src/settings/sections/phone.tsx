// Связь с телефоном (протокол KDE Connect): устройства, сопряжение, что передавать.

import { useState } from "react";
import { BatteryCharging, BatteryMedium, BellRing, Plus, Smartphone, Trash2 } from "lucide-react";
import { Button, IconButton, Note, Toggle } from "../../components/ui";
import { useAction, useEvent, useLoad } from "../../hooks/common";
import { ipc } from "../../services/ipc";
import { useApp } from "../../stores/app";
import type { PhoneDevice, PhoneStatus } from "../../types";
import { Field, Group, Page } from "../components";

/** Свои команды: название и что открыть (программа, файл или сайт). */
function CustomCommands({ list, onChange }: { list: { name: string; target: string }[]; onChange: (v: { name: string; target: string }[]) => void }) {
  const [name, setName] = useState("");
  const [target, setTarget] = useState("");
  const add = () => {
    if (!name.trim() || !target.trim()) return;
    onChange([...list, { name: name.trim(), target: target.trim() }]);
    setName("");
    setTarget("");
  };
  return (
    <div className="field wide">
      <div className="field-text" style={{ width: "100%" }}>
        <div className="field-label">Свои команды</div>
        <div className="field-desc">Открыть программу, файл или сайт одной кнопкой на телефоне</div>
        {list.map((c, i) => (
          <div key={i} className="row gap" style={{ marginTop: 8 }}>
            <b style={{ minWidth: 140 }}>{c.name}</b>
            <span className="grow small muted ellipsis">{c.target}</span>
            <IconButton size="sm" title="Удалить команду" onClick={() => onChange(list.filter((_, j) => j !== i))}>
              <Trash2 size={13} />
            </IconButton>
          </div>
        ))}
        <div className="row gap" style={{ marginTop: 10 }}>
          <input className="input" style={{ width: 160 }} placeholder="Название" value={name} maxLength={40} onChange={(e) => setName(e.target.value)} />
          <input className="input grow" placeholder="Путь к программе или https://…" value={target} maxLength={500} onChange={(e) => setTarget(e.target.value)} onKeyDown={(e) => e.key === "Enter" && add()} />
          <Button disabled={!name.trim() || !target.trim()} onClick={add}>
            <Plus size={14} /> Добавить
          </Button>
        </div>
      </div>
    </div>
  );
}

function DeviceRow({ d }: { d: PhoneDevice }) {
  const { error, run } = useAction();
  const state = d.pairing === "incoming" ? "просит подключиться" : d.pairing === "requested" ? "ждём подтверждения на телефоне" : d.reachable ? "в сети" : "не в сети";
  return (
    <div className="field phone-row">
      <span className="phone-icon">
        <Smartphone size={18} />
      </span>
      <div className="field-text">
        <div className="field-label">{d.name}</div>
        <div className="field-desc">
          <span className={`phone-dot ${d.reachable ? "on" : ""}`} />
          {state}
          {d.battery != null && (
            <span className="phone-battery">
              {d.charging ? <BatteryCharging size={13} /> : <BatteryMedium size={13} />}
              <span className="mono">{d.battery}%</span>
            </span>
          )}
        </div>
        {error && <div className="field-desc" style={{ color: "var(--danger)" }}>{error}</div>}
      </div>
      <div className="field-control row gap">
        {d.pairing === "incoming" ? (
          <>
            <Button variant="primary" onClick={() => run(() => ipc.phoneAccept(d.id, true))}>Подключить</Button>
            <Button onClick={() => run(() => ipc.phoneAccept(d.id, false))}>Отклонить</Button>
          </>
        ) : d.paired ? (
          <>
            {d.reachable && (
              <Button onClick={() => run(() => ipc.phoneRing(d.id))} title="Телефон зазвонит, даже в беззвучном режиме">
                <BellRing size={14} /> Найти
              </Button>
            )}
            <Button onClick={() => run(() => ipc.phoneUnpair(d.id))}>Отвязать</Button>
          </>
        ) : (
          <Button variant="primary" disabled={d.pairing === "requested"} onClick={() => run(() => ipc.phonePair(d.id))}>
            Связать
          </Button>
        )}
      </div>
    </div>
  );
}

export function PhoneSection() {
  const p = useApp((x) => x.settings!.phone);
  const update = useApp((x) => x.update);
  const set = (patch: Partial<typeof p>) => update("phone", patch).catch(() => {});
  const status = useLoad(() => ipc.phoneStatus());
  useEvent<PhoneStatus>("phone://status", (s) => status.setData(s));
  const s = status.data;
  const devices = s?.devices ?? [];

  return (
    <Page title="Телефон">
      <Group>
        <Field label="Связь с телефоном" description={s ? (s.enabled ? s.message || `Компьютер виден как «${s.deviceName}»` : "Выключена") : undefined}>
          <Toggle checked={p.enabled} onChange={(enabled) => set({ enabled })} />
        </Field>
      </Group>

      {p.enabled && (
        <Group title="Устройства">
          {devices.length === 0 ? (
            <div className="field">
              <div className="field-text">
                <div className="field-label">Телефон пока не найден</div>
                <div className="field-desc">
                  Установите на телефон приложение KDE Connect (Google Play, F-Droid или App Store) и подключитесь к той же Wi‑Fi, что и
                  компьютер. Компьютер появится в списке на телефоне, а телефон — здесь
                </div>
              </div>
            </div>
          ) : (
            devices.map((d) => <DeviceRow key={d.id} d={d} />)
          )}
        </Group>
      )}

      {p.enabled && (
        <Group title="Что передавать">
          <Field label="Уведомления телефона" description="Сообщения из мессенджеров и других приложений, с группировкой">
            <Toggle checked={p.notifications} onChange={(notifications) => set({ notifications })} />
          </Field>
          <Field label="Звонки" description="Входящий звонок с кнопкой «Без звука», пропущенные">
            <Toggle checked={p.calls} onChange={(calls) => set({ calls })} />
          </Field>
          <Field label="Общий буфер обмена" description="Скопировали на телефоне — вставляете на компьютере, и наоборот">
            <Toggle checked={p.clipboard} onChange={(clipboard) => set({ clipboard })} />
          </Field>
          <Field label="Низкий заряд" description="Предупреждение в острове, когда телефон разряжается">
            <Toggle checked={p.battery} onChange={(battery) => set({ battery })} />
          </Field>
          <Field label="Закрывать уведомления и на телефоне" description="Закрыли уведомление в острове — оно пропадает и из шторки телефона">
            <Toggle checked={p.syncDismiss ?? true} onChange={(syncDismiss) => set({ syncDismiss })} />
          </Field>
          <Field label="SMS" description="Переписка с телефона и ответы с компьютера — в карточке «Телефон». На телефоне разрешите KDE Connect доступ к SMS и контактам">
            <Toggle checked={p.sms ?? true} onChange={(sms) => set({ sms })} />
          </Field>
          <Field label="Музыка с телефона" description="Что играет на телефоне — в острове, с обложкой и кнопками">
            <Toggle checked={p.music ?? true} onChange={(music) => set({ music })} />
          </Field>
        </Group>
      )}

      {p.enabled && (
        <Group title="Телефон управляет компьютером">
          <Field label="Тачпад, клавиатура и пульт для презентаций" description="В KDE Connect на телефоне: «Удалённый ввод» и «Презентация»">
            <Toggle checked={p.remoteInput ?? true} onChange={(remoteInput) => set({ remoteInput })} />
          </Field>
          <Field label="Кнопки-команды" description="В KDE Connect на телефоне: «Выполнить команду» — заблокировать ПК, сон, выключить экран, звук, музыка, «Не беспокоить», показать остров">
            <Toggle checked={p.commands ?? true} onChange={(commands) => set({ commands })} />
          </Field>
          {(p.commands ?? true) && <CustomCommands list={p.customCommands ?? []} onChange={(customCommands) => set({ customCommands })} />}
        </Group>
      )}

      {p.enabled && (
        <Note kind="info">
          Всё работает только в вашей локальной сети и только с телефонами, которые вы связали. Если Windows спросит доступ к сети для
          Desktop Island — разрешите для частных сетей
        </Note>
      )}
    </Page>
  );
}
