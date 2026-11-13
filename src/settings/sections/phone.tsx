// Связь с телефоном (протокол KDE Connect): устройства, сопряжение, что передавать.

import { BatteryCharging, BatteryMedium, BellRing, Smartphone } from "lucide-react";
import { Button, Note, Toggle } from "../../components/ui";
import { useAction, useEvent, useLoad } from "../../hooks/common";
import { ipc } from "../../services/ipc";
import { useApp } from "../../stores/app";
import type { PhoneDevice, PhoneStatus } from "../../types";
import { Field, Group, Page } from "../components";

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
