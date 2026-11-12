// Последний снимок системных показателей и короткая история для графиков.

import { create } from "zustand";
import type { SysSnapshot } from "../types";

interface SysStore {
  snapshot: SysSnapshot | null;
  cpu: number[];
  ram: number[];
  gpu: number[];
  rx: number[];
  tx: number[];
  push: (s: SysSnapshot, limit: number) => void;
}

const append = (arr: number[], v: number, limit: number) => {
  const next = arr.length >= limit ? arr.slice(arr.length - limit + 1) : arr.slice();
  next.push(v);
  return next;
};

export const useSys = create<SysStore>((set) => ({
  snapshot: null,
  cpu: [],
  ram: [],
  gpu: [],
  rx: [],
  tx: [],
  push: (s, limit) =>
    set((st) => ({
      snapshot: s,
      cpu: append(st.cpu, s.cpu, limit),
      ram: append(st.ram, s.ramTotal ? (s.ramUsed / s.ramTotal) * 100 : 0, limit),
      gpu: s.gpu.usage != null ? append(st.gpu, s.gpu.usage, limit) : st.gpu,
      rx: append(st.rx, s.netRxBps ?? 0, limit),
      tx: append(st.tx, s.netTxBps ?? 0, limit),
    })),
}));
