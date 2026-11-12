// Раскрытое состояние: минимальный заголовок (название панели, точки-индикаторы,
// «по центру», настройки) и содержимое активной панели. Панели листаются
// колесом мыши или кликом по точкам, с направленным сдвигом.

import { Suspense, useLayoutEffect, useRef } from "react";
import { AnimatePresence, motion } from "motion/react";
import { Crosshair, Settings2 } from "lucide-react";
import { metaFor } from "../cards";
import { ipc } from "../services/ipc";
import { useApp } from "../stores/app";
import { useIsland } from "../stores/island";
import type { CardInfo } from "../types";
import { IconButton, Spinner } from "./ui";
import { ErrorBoundary } from "./ErrorBoundary";

let lastIndex = 0;

export function ExpandedView({ panels, onHeight }: { panels: CardInfo[]; onHeight: (h: number) => void }) {
  const activeCard = useIsland((s) => s.activeCard);
  const animations = useApp((s) => s.settings?.appearance.animations);
  const innerRef = useRef<HTMLDivElement>(null);
  const index = Math.max(0, panels.findIndex((c) => c.id === activeCard));
  const active = panels[index];
  const meta = active ? metaFor(active.id) : null;
  const Card = meta?.component;
  const dir = index >= lastIndex ? 1 : -1;
  lastIndex = index;
  const slide = animations === "off" ? 0 : 22;

  // Высота по естественному размеру содержимого (а не контейнера) — окно не «раздувается».
  useLayoutEffect(() => {
    const el = innerRef.current;
    if (!el) return;
    const report = () => onHeight(Math.ceil(el.getBoundingClientRect().height));
    report();
    const ro = new ResizeObserver(report);
    ro.observe(el);
    return () => ro.disconnect();
  }, [active?.id, onHeight]);

  return (
    <div className="expanded">
      <div className="expanded-head">
        <span className="panel-title">{active?.title ?? ""}</span>
        <div className="dots" role="tablist">
          {panels.map((c) => (
            <button
              key={c.id}
              type="button"
              role="tab"
              aria-selected={c.id === active?.id}
              aria-label={c.title}
              title={c.title}
              className={`dot-btn ${c.id === active?.id ? "on" : ""}`}
              onClick={() => useIsland.getState().setCard(c.id)}
            />
          ))}
        </div>
        <div className="head-actions">
          <IconButton size="sm" title="Поставить панель по центру" onClick={() => ipc.islandCenter()}>
            <Crosshair size={14} />
          </IconButton>
          <IconButton size="sm" title="Настройки" onClick={() => ipc.openSettings(active ? sectionFor(active.id) : undefined)}>
            <Settings2 size={14} />
          </IconButton>
        </div>
      </div>
      <div className="expanded-content">
        <AnimatePresence mode="popLayout" initial={false} custom={dir}>
          <motion.div
            key={active?.id ?? "none"}
            ref={innerRef}
            className="panel"
            custom={dir}
            initial={{ opacity: 0, x: dir * slide, filter: "blur(4px)" }}
            animate={{ opacity: 1, x: 0, filter: "blur(0px)" }}
            exit={{ opacity: 0, x: -dir * slide, transition: { duration: 0.1 } }}
            transition={{ type: "spring", stiffness: 500, damping: 40 }}
          >
            {!active ? (
              <div className="card-pad muted">Все панели скрыты. Включите их в «Настройки → Карточки».</div>
            ) : Card ? (
              <ErrorBoundary key={active.id} title={active.title}>
                <Suspense fallback={<div className="card-pad center"><Spinner /></div>}>
                  <Card cardId={active.id} pluginId={active.pluginId} />
                </Suspense>
              </ErrorBoundary>
            ) : null}
          </motion.div>
        </AnimatePresence>
      </div>
    </div>
  );
}

function sectionFor(cardId: string): string {
  const map: Record<string, string> = {
    music: "music",
    notifications: "notifications",
    system: "system-monitor",
    clipboard: "clipboard",
    apps: "integrations",
    ai: "integrations",
    integrations: "integrations",
  };
  return cardId.startsWith("plugin:") ? "plugins" : (map[cardId] ?? "cards");
}
