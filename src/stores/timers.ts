import { create } from "zustand";
import { ipc } from "../services/ipc";
import type { Timer } from "../types";

interface TimersStore {
  timers: Timer[];
  loaded: boolean;
  set: (t: Timer[]) => void;
  load: () => Promise<void>;
}

export const useTimers = create<TimersStore>((set) => ({
  timers: [],
  loaded: false,
  set: (timers) => set({ timers, loaded: true }),
  load: async () => {
    try {
      set({ timers: await ipc.timers(), loaded: true });
    } catch {
      set({ loaded: true });
    }
  },
}));

export function timerElapsed(t: Timer, now: number): number {
  return t.elapsedMs + (t.startedAt ? Math.max(0, now - t.startedAt) : 0);
}

export function timerRemaining(t: Timer, now: number): number {
  return Math.max(0, t.durationMs - timerElapsed(t, now));
}
