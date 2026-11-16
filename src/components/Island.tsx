// Остров: «чёлка», прилегающая к верхнему краю экрана (или «пилюля», если её
// перетащили ниже). Одна критически задемпфированная пружина анимирует ширину,
// высоту и углы — форма плавно «перетекает» без перелёта. Окно подгоняется
// точно под содержимое: при увеличении — сразу, при уменьшении — после анимации.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AnimatePresence, motion, type Transition } from "motion/react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { CompactView } from "./CompactView";
import { ExpandedView } from "./ExpandedView";
import { NotificationView } from "./NotificationView";
import { ipc } from "../services/ipc";
import { useApp } from "../stores/app";
import { useActiveTheme } from "../hooks/useTheme";
import { pickCompact, useIsland } from "../stores/island";
import { playSound, stopSound } from "../utils/sounds";

/** Поле по бокам окна под вогнутые «мостики» к краю экрана. */
const SIDE = 14;
const COMPACT_H = 34;
const HEADER_H = 42;
const MAX_EXPANDED_H = 520;

function useShellSpring(): { shell: Transition; content: Transition; ms: number } {
  const a = useApp((s) => s.settings?.appearance);
  const speed = a?.animationSpeed ?? 1;
  if (!a || a.animations === "off") return { shell: { duration: 0 }, content: { duration: 0 }, ms: 0 };
  if (a.animations === "reduced") return { shell: { duration: 0.16 / speed, ease: "easeOut" }, content: { duration: 0.1 }, ms: 170 };
  // Как в Nectar: критически задемпфированная пружина — быстро и без «пружинистого» перелёта.
  return {
    shell: { type: "spring", stiffness: 400 * speed, damping: 40, mass: 1 },
    content: { type: "spring", stiffness: 500 * speed, damping: 34 },
    ms: Math.round(360 / speed),
  };
}

/** Окно острова постоянного размера: самый большой вид острова плюс поле под тень.
 *  Меняется только форма внутри — WebView не перестраивается на каждом кадре,
 *  поэтому остров не дёргается. */
const FRAME_W = 384;
const SHADOW = 24;

function useFixedFrame(scale: number) {
  useEffect(() => {
    ipc.islandResize(Math.ceil(FRAME_W * scale + SIDE * 2), Math.ceil(MAX_EXPANDED_H * scale + SHADOW)).catch(() => {});
  }, [scale]);
}

/** Область, где остров принимает мышь (остальное окно пропускает клики насквозь).
 *  При росте — сразу, при уменьшении — после анимации, чтобы край не «отрезал» клик. */
function useHitbox(width: number, height: number, settleMs: number) {
  const last = useRef({ w: 0, h: 0 });
  const timer = useRef<number | undefined>(undefined);
  useEffect(() => {
    const W = Math.ceil(width);
    const H = Math.ceil(height + 1);
    window.clearTimeout(timer.current);
    const prev = last.current;
    if (W === prev.w && H === prev.h) return;
    const grow = { w: Math.max(W, prev.w), h: Math.max(H, prev.h) };
    if (grow.w !== prev.w || grow.h !== prev.h) {
      last.current = grow;
      ipc.islandHitbox(grow.w, grow.h).catch(() => {});
    }
    if (grow.w !== W || grow.h !== H) {
      timer.current = window.setTimeout(() => {
        last.current = { w: W, h: H };
        ipc.islandHitbox(W, H).catch(() => {});
      }, settleMs + 40);
    }
  }, [width, height, settleMs]);
}

export function Island() {
  const mode = useIsland((s) => s.mode);
  const activeCard = useIsland((s) => s.activeCard);
  const hovered = useIsland((s) => s.hovered);
  const attached = useIsland((s) => s.attached);
  const activities = useIsland((s) => s.activities);
  const notification = useIsland((s) => s.notification.current);
  const settings = useApp((s) => s.settings)!;
  const cards = useApp((s) => s.cards);
  const { shell, content, ms } = useShellSpring();
  const scale = settings.appearance.scale;
  const islandStyle = settings.appearance.islandStyle ?? "solid";
  const themeShape = useActiveTheme("island")?.shape;
  // «Аура»: поворот градиента 10 раз в секунду (дёшево для процессора), пауза без анимаций.
  const auraRef = useRef<HTMLSpanElement>(null);
  const animationsOn = settings.appearance.animations !== "off";
  useEffect(() => {
    if (islandStyle !== "aura" || !animationsOn || window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    let angle = 0;
    const id = window.setInterval(() => {
      if (document.hidden) return;
      angle = (angle + 4.5) % 360;
      auraRef.current?.style.setProperty("--aura-angle", `${angle}deg`);
    }, 100);
    return () => window.clearInterval(id);
  }, [islandStyle, animationsOn]);
  const [contentH, setContentH] = useState(200);

  // Видимые панели + временно открытая скрытая (например, поиск по горячей клавише).
  const panels = useMemo(() => {
    const visible = cards.filter((c) => c.enabled && c.moduleEnabled);
    const extra = cards.find((c) => c.id === activeCard && c.moduleEnabled && !c.enabled);
    return extra ? [...visible, extra] : visible;
  }, [cards, activeCard]);

  const top = pickCompact(activities);
  const greeting = useIsland((s) => s.greeting);
  const strip = useIsland((s) => s.strip);
  const wide = !!greeting || (!!top && (top.kind === "music" ? settings.music.showInCompact : true));

  const size = useMemo(() => {
    if (mode === "expanded") return { w: 384 * scale, h: Math.min(MAX_EXPANDED_H, HEADER_H + contentH) * scale };
    if (mode === "notification" && notification) {
      // Подсказка клавиатуры — небольшая «пилюля», а не карточка уведомления.
      if (notification.source === "kbd") return { w: (notification.title.startsWith("Caps") ? 236 : 200) * scale, h: 44 * scale };
      const stackLines = Math.min(2, notification.stack?.length ?? 0);
      return { w: 360 * scale, h: ((notification.actions.length ? 104 : 76) + stackLines * 17) * scale };
    }
    // Полноэкранное видео: тонкая полоска, по наведению — обычный остров.
    if (strip && !hovered && !greeting) return { w: 96 * scale, h: 6 * scale };
    const w = (wide ? 280 : settings.systemMonitor.showInCompact ? 244 : 200) * scale;
    return hovered ? { w: w + 12 * scale, h: (COMPACT_H + 2) * scale } : { w, h: COMPACT_H * scale };
  }, [mode, contentH, notification, wide, hovered, scale, settings.systemMonitor.showInCompact, strip, greeting]);

  // Овальная форма: в свёрнутом виде — полная «пилюля», раскрытый остров — крупные скругления.
  const kbdNotice = mode === "notification" && notification?.source === "kbd";
  // Скругление из своей темы (по умолчанию 30): у пилюли — не больше половины высоты.
  const r = themeShape?.island ?? 30;
  const radius = mode === "expanded" ? r * scale : mode === "notification" && !kbdNotice ? Math.max(6, r - 2) * scale : Math.min(size.h / 2, r === 30 ? size.h / 2 : r * scale);
  const topRadius = attached ? 0 : radius;

  useFixedFrame(scale);
  useHitbox(size.w, size.h, ms);

  // Свой звук острова при новом уведомлении (системный приглушён режимом «Не беспокоить»).
  // Окончание таймера — отдельный сигнал-будильник: звучит, пока уведомление не закроют
  // (или заданное время). Звуки одного типа не накладываются — у каждого свой канал.
  const notifId = notification?.id;
  const isTimer = notification?.kind === "timer";
  const silent = notification?.source === "kbd";
  const sounds = settings.sounds;
  useEffect(() => {
    if (!notifId) return;
    if (isTimer) {
      if (!sounds.timerEnabled) return;
      playSound("timer", sounds.timer, sounds.timerVolume, { loop: true, limitSecs: sounds.timerLimitSecs || undefined });
      return () => stopSound("timer");
    }
    if (!silent && settings.notifications.sound && !settings.notifications.dnd) playSound("notification", sounds.notification, sounds.notificationVolume);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [notifId]);
  // Раскрыли остров во время сигнала — это тоже «остановить».
  useEffect(() => {
    if (mode === "expanded") stopSound("timer");
  }, [mode]);

  useEffect(() => {
    const active = mode === "expanded" && activeCard ? [activeCard] : [];
    ipc.setActiveCards(active, mode === "expanded").catch(() => {});
  }, [mode, activeCard]);

  useEffect(() => {
    if (mode === "expanded" && activeCard !== "lyrics" && (!activeCard || !panels.some((c) => c.id === activeCard))) {
      const preferred = top?.kind === "music" ? "music" : top?.id.startsWith("timer:") ? "timers" : panels[0]?.id;
      if (preferred && panels.some((p) => p.id === preferred)) useIsland.getState().setCard(preferred);
      else if (panels[0]) useIsland.getState().setCard(panels[0].id);
    }
  }, [mode, activeCard, panels, top]);

  // Запуск приложения: панель сворачивается, чтобы показать анимацию запуска.
  const launchKey = top?.id === "launch" ? top.updatedAt : 0;
  useEffect(() => {
    if (launchKey && useIsland.getState().mode === "expanded") useIsland.getState().collapse();
  }, [launchKey]);

  // Перетаскивание файлов на остров → полка. Файлы добавляет сам бэкенд,
  // здесь только подсветка и показ полки.
  useEffect(() => {
    const un = listen<{ state: "enter" | "leave" | "drop"; added?: number }>("shelf://drag", (e) => {
      const st = useIsland.getState();
      const s = e.payload.state;
      if (s === "enter") {
        st.setDragging(true);
        st.expand("shelf");
      } else if (s === "drop") {
        st.setDragging(false);
        if (st.mode !== "expanded" || st.activeCard !== "shelf") st.expand("shelf");
      } else {
        st.setDragging(false);
      }
    });
    return () => {
      un.then((u) => u());
    };
  }, []);

  // Сворачивание при потере фокуса и по Esc.
  useEffect(() => {
    const un = getCurrentWindow().onFocusChanged(({ payload: focused }) => {
      const st = useIsland.getState();
      if (!focused && st.mode === "expanded") st.collapse();
    });
    const key = (e: KeyboardEvent) => e.key === "Escape" && useIsland.getState().collapse();
    window.addEventListener("keydown", key);
    return () => {
      un.then((u) => u());
      window.removeEventListener("keydown", key);
    };
  }, []);

  // Перетаскивание: при движении > 4 px — системное перемещение окна; клик — раскрытие.
  const drag = useRef<{ x: number; y: number; moved: boolean } | null>(null);
  const onPointerDown = useCallback((e: React.PointerEvent) => {
    if (e.button !== 0) return;
    if ((e.target as HTMLElement).closest("button, input, a, textarea, select, iframe, [data-no-drag]")) return;
    drag.current = { x: e.screenX, y: e.screenY, moved: false };
  }, []);
  const onPointerMove = useCallback(
    (e: React.PointerEvent) => {
      const d = drag.current;
      if (!d || d.moved || (e.buttons & 1) === 0) return;
      if (Math.abs(e.screenX - d.x) + Math.abs(e.screenY - d.y) > 4) {
        d.moved = true;
        if (!settings.position.locked) {
          useIsland.getState().setHovered(false);
          getCurrentWindow().startDragging().catch(() => {});
        }
      }
    },
    [settings.position.locked],
  );
  const onPointerUp = useCallback(() => {
    const d = drag.current;
    drag.current = null;
    if (!d || d.moved) return;
    const st = useIsland.getState();
    if (st.mode === "compact") st.expand(null);
    else if (st.mode === "notification") st.expand("notifications");
  }, []);

  // Колесо мыши над панелью переключает панели (как в Nectar).
  const wheel = useRef({ acc: 0, t: 0 });
  const onWheel = useCallback(
    (e: React.WheelEvent) => {
      if (mode !== "expanded" || panels.length < 2) return;
      // Внутри прокручиваемого списка колесо прокручивает список.
      const scroller = (e.target as HTMLElement).closest(".scroll, .expanded-content");
      if (scroller && scroller.scrollHeight > scroller.clientHeight + 2) return;
      const now = performance.now();
      if (now - wheel.current.t > 180) wheel.current.acc = 0;
      wheel.current.t = now;
      wheel.current.acc += Math.abs(e.deltaY) > Math.abs(e.deltaX) ? e.deltaY : e.deltaX;
      if (Math.abs(wheel.current.acc) < 60) return;
      const dir = wheel.current.acc > 0 ? 1 : -1;
      wheel.current.acc = 0;
      const i = panels.findIndex((p) => p.id === activeCard);
      const next = panels[(Math.max(0, i) + dir + panels.length) % panels.length];
      useIsland.getState().setCard(next.id);
    },
    [mode, panels, activeCard],
  );

  return (
    <div className="island-root" style={{ paddingLeft: SIDE, paddingRight: SIDE }}>
      <motion.div
        className={`island mode-${mode} style-${islandStyle} ${attached ? "attached" : "floating"} tone-${settings.appearance.islandTone ?? "graphite"} ${strip && !hovered && mode === "compact" && !greeting ? "is-strip" : ""}`}
        initial={false}
        animate={{
          width: size.w,
          height: size.h,
          borderBottomLeftRadius: radius,
          borderBottomRightRadius: radius,
          borderTopLeftRadius: topRadius,
          borderTopRightRadius: topRadius,
        }}
        transition={shell}
        onPointerDown={onPointerDown}
        onPointerMove={(e) => {
          // Блик стекла и графита следует за курсором.
          if (islandStyle === "glass" || islandStyle === "graphite") {
            const b = e.currentTarget.getBoundingClientRect();
            e.currentTarget.style.setProperty("--mx", `${e.clientX - b.left}px`);
            e.currentTarget.style.setProperty("--my", `${e.clientY - b.top}px`);
          }
          onPointerMove(e);
        }}
        onPointerUp={onPointerUp}
        onWheel={onWheel}
        onMouseEnter={() => useIsland.getState().setHovered(true)}
        onMouseLeave={() => useIsland.getState().setHovered(false)}
        onContextMenu={(e) => {
          e.preventDefault();
          ipc.openSettings().catch(() => {});
        }}
      >
        <span className="island-aura" aria-hidden ref={auraRef} />
        <span className="island-sheen" aria-hidden />
        <div className="island-clip">
          <AnimatePresence mode="wait" initial={false}>
            {mode === "expanded" ? (
              <motion.div key="expanded" className="layer" initial={{ opacity: 0, filter: "blur(6px)" }} animate={{ opacity: 1, filter: "blur(0px)" }} exit={{ opacity: 0, transition: { duration: 0.08 } }} transition={content}>
                <ExpandedView panels={panels} onHeight={setContentH} />
              </motion.div>
            ) : mode === "notification" && notification ? (
              <motion.div key={`n-${notification.id}`} className="layer" initial={{ opacity: 0, filter: "blur(6px)", y: -4 }} animate={{ opacity: 1, filter: "blur(0px)", y: 0 }} exit={{ opacity: 0, transition: { duration: 0.08 } }} transition={content}>
                <NotificationView n={notification} />
              </motion.div>
            ) : (
              <motion.div key="compact" className="layer" initial={{ opacity: 0, filter: "blur(4px)" }} animate={{ opacity: 1, filter: "blur(0px)" }} exit={{ opacity: 0, transition: { duration: 0.06 } }} transition={content}>
                <CompactView top={top} />
              </motion.div>
            )}
          </AnimatePresence>
        </div>
      </motion.div>
    </div>
  );
}
