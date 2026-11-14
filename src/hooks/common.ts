// Небольшие общие хуки.

import { useCallback, useEffect, useRef, useState } from "react";
import type { Transition } from "motion/react";
import { errorMessage, on } from "../services/ipc";
import { useApp } from "../stores/app";
import { useIsland } from "../stores/island";

/** Текущее время, обновляемое с заданным интервалом. Останавливается в игровом режиме. */
export function useNow(intervalMs = 1000, enabled = true): number {
  const [now, setNow] = useState(Date.now());
  const paused = useIsland((s) => s.gameMode.active);
  useEffect(() => {
    if (!enabled || paused) return;
    setNow(Date.now());
    const id = window.setInterval(() => setNow(Date.now()), intervalMs);
    return () => window.clearInterval(id);
  }, [intervalMs, enabled, paused]);
  return now;
}

/** Подписка на событие бэкенда на время жизни компонента. */
export function useEvent<P>(event: string, handler: (p: P) => void) {
  const ref = useRef(handler);
  ref.current = handler;
  useEffect(() => {
    let un: (() => void) | undefined;
    let cancelled = false;
    on<P>(event, (p) => ref.current(p)).then((u) => {
      if (cancelled) u();
      else un = u;
    });
    return () => {
      cancelled = true;
      un?.();
    };
  }, [event]);
}

/** Загрузка данных с обработкой ошибок; `reload` — повторная загрузка. */
export function useLoad<T>(loader: () => Promise<T>, deps: unknown[] = []) {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const mounted = useRef(true);
  const reload = useCallback(async (): Promise<T | null> => {
    setLoading(true);
    try {
      const d = await loader();
      if (mounted.current) {
        setData(d);
        setError(null);
      }
      return d;
    } catch (e) {
      if (mounted.current) setError(errorMessage(e));
      return null;
    } finally {
      if (mounted.current) setLoading(false);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
  useEffect(() => {
    mounted.current = true;
    reload();
    return () => {
      mounted.current = false;
    };
  }, [reload]);
  return { data, error, loading, reload, setData };
}

/** Выполнение действия с перехватом ошибки для показа пользователю. */
export function useAction() {
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const run = useCallback(async <R,>(fn: () => Promise<R>): Promise<R | undefined> => {
    setBusy(true);
    setError(null);
    try {
      return await fn();
    } catch (e) {
      setError(errorMessage(e));
      return undefined;
    } finally {
      setBusy(false);
    }
  }, []);
  return { error, busy, run, setError };
}

/** Параметры анимации по настройкам пользователя. */
export function useTransition(kind: "layout" | "fade" = "layout"): Transition {
  const a = useApp((s) => s.settings?.appearance);
  const speed = a?.animationSpeed ?? 1;
  if (!a || a.animations === "off") return { duration: 0 };
  if (a.animations === "reduced") return { duration: 0.14 / speed, ease: "easeOut" };
  if (kind === "fade") return { duration: 0.22 / speed, ease: "easeOut" };
  return { type: "spring", stiffness: 420 * speed, damping: 34, mass: 0.9 };
}

/** Интервал опроса, активный только пока компонент смонтирован и не в игровом режиме. */
export function usePoll(fn: () => void, intervalMs: number, enabled = true) {
  const ref = useRef(fn);
  ref.current = fn;
  const paused = useIsland((s) => s.gameMode.active);
  useEffect(() => {
    if (!enabled || paused) return;
    ref.current();
    const id = window.setInterval(() => ref.current(), intervalMs);
    return () => window.clearInterval(id);
  }, [intervalMs, enabled, paused]);
}
