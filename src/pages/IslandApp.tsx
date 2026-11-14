// Окно панели: инициализация, подписки на события бэкенда, отображение острова.

import { useEffect } from "react";
import { MotionConfig } from "motion/react";
import { Island } from "../components/Island";
import { useTheme } from "../hooks/useTheme";
import { events, ipc, on } from "../services/ipc";
import { useApp } from "../stores/app";
import { useIsland } from "../stores/island";
import { useMedia } from "../stores/media";
import { useSys } from "../stores/sysmon";
import { useTimers } from "../stores/timers";
import type { Activity, GameModeState, KbdState, MediaSession, NotificationPayload, PrivacyState, SysSnapshot, Timer } from "../types";
import "../styles/island.css";
import { greetingText, playVoice } from "../utils/voice";

export default function IslandApp() {
  const ready = useApp((s) => s.ready);
  const animations = useApp((s) => s.settings?.appearance.animations);
  useTheme("island");

  useEffect(() => {
    const unsubs: Promise<() => void>[] = [];
    const island = useIsland.getState();
    unsubs.push(on<NotificationPayload>(events.NOTIFICATION, (p) => useIsland.getState().setNotification(p)));
    unsubs.push(on<Activity[]>(events.ACTIVITIES, (a) => useIsland.getState().setActivities(a)));
    unsubs.push(on<GameModeState>(events.GAME_MODE, (g) => useIsland.getState().setGameMode(g)));
    unsubs.push(on<MediaSession[]>(events.MEDIA_SESSIONS, (s) => useMedia.getState().setSessions(s)));
    unsubs.push(on<Timer[]>(events.TIMERS_CHANGED, (t) => useTimers.getState().set(t)));
    unsubs.push(on<boolean>("island://attached", (a) => useIsland.getState().setAttached(a)));
    unsubs.push(on<PrivacyState>("privacy://state", (p) => useIsland.getState().setPrivacy(p)));
    unsubs.push(on<string>("voice://play", (name) => playVoice(name).catch(() => {})));
    unsubs.push(on<boolean>("island://strip", (s) => useIsland.getState().setStrip(s)));
    ipc.islandStrip().then((s) => useIsland.getState().setStrip(s)).catch(() => {});
    unsubs.push(on<KbdState>(events.KBD, (k) => useIsland.getState().setKbd(k)));
    ipc.kbdState().then((k) => useIsland.getState().setKbd(k)).catch(() => {});
    let greetTimer: number | undefined;
    unsubs.push(
      on<string>("island://greet", (reason) => {
        if (useApp.getState().settings?.appearance.greeting === false) return;
        window.clearTimeout(greetTimer);
        useIsland.getState().setGreeting(greetingText(reason));
        greetTimer = window.setTimeout(() => useIsland.getState().setGreeting(null), 3600);
      }),
    );
    ipc.privacyState().then((p) => useIsland.getState().setPrivacy(p)).catch(() => {});
    ipc.islandAttached().then((a) => useIsland.getState().setAttached(a)).catch(() => {});
    unsubs.push(
      on<SysSnapshot>(events.SYSMON_SNAPSHOT, (s) => useSys.getState().push(s, useApp.getState().settings?.systemMonitor.historyPoints ?? 60)),
    );
    unsubs.push(
      on<{ action: string; card?: string }>(events.COMMAND, (c) => {
        const st = useIsland.getState();
        if (c.action === "expand") st.mode === "expanded" ? st.collapse() : st.expand();
        if (c.action === "open-card" && c.card) st.expand(c.card);
      }),
    );

    useApp
      .getState()
      .init()
      .then(async () => {
        const app = useApp.getState();
        const b = await ipc.bootstrap();
        island.setNotification(b.notification);
        island.setActivities(b.activities);
        island.setGameMode(b.gameMode);
        if (app.settings?.modules.music) ipc.mediaSessions().then((s) => useMedia.getState().setSessions(s)).catch(() => {});
        if (app.settings?.modules.timers) useTimers.getState().load();
        // Показываем окно только после первой отрисовки — без белой вспышки.
        // WebKitGTK (Linux) не вызывает requestAnimationFrame, пока окно скрыто,
        // поэтому дублируем таймером — иначе окно так и не появится.
        let sent = false;
        const ready = () => {
          if (sent) return;
          sent = true;
          ipc.islandReady().catch(() => {});
        };
        requestAnimationFrame(ready);
        setTimeout(ready, 250);
      })
      .catch((e) => {
        console.error("bootstrap failed", e);
        // Даже при ошибке загрузки окно должно появиться, чтобы было видно, что не так.
        ipc.islandReady().catch(() => {});
      });

    return () => {
      unsubs.forEach((p) => p.then((u) => u()));
    };
  }, []);

  if (!ready) return null;
  return (
    <MotionConfig reducedMotion={animations === "full" ? "never" : "always"}>
      <Island />
    </MotionConfig>
  );
}
