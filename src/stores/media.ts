// Медиасессии и кэш обложек (по ключу трека, ограниченный размер).

import { create } from "zustand";
import { ipc } from "../services/ipc";
import type { MediaSession } from "../types";

const MAX_ART = 8;

interface MediaStore {
  sessions: MediaSession[];
  selectedId: string | null;
  artwork: Record<string, string | null>;
  artOrder: string[];
  setSessions: (s: MediaSession[]) => void;
  select: (id: string | null) => void;
  loadArtwork: (s: MediaSession) => Promise<void>;
}

const pending = new Set<string>();

export const useMedia = create<MediaStore>((set, get) => ({
  sessions: [],
  selectedId: null,
  artwork: {},
  artOrder: [],
  setSessions: (sessions) => set({ sessions }),
  select: (id) => {
    set({ selectedId: id });
    ipc.mediaSelect(id).catch(() => {});
  },
  loadArtwork: async (s) => {
    const key = s.artworkKey;
    if (!key || key in get().artwork || pending.has(key)) return;
    if (s.artworkUrl) {
      set((st) => remember(st, key, s.artworkUrl));
      return;
    }
    pending.add(key);
    try {
      const url = await ipc.mediaArtwork(s.id);
      set((st) => remember(st, key, url));
    } catch {
      set((st) => remember(st, key, null));
    } finally {
      pending.delete(key);
    }
  },
}));

function remember(st: MediaStore, key: string, url: string | null) {
  const order = [...st.artOrder.filter((k) => k !== key), key];
  const artwork = { ...st.artwork, [key]: url };
  while (order.length > MAX_ART) {
    const old = order.shift()!;
    delete artwork[old];
  }
  return { artwork, artOrder: order };
}

export function currentSession(sessions: MediaSession[], selectedId: string | null): MediaSession | null {
  return (
    sessions.find((s) => s.id === selectedId) ??
    sessions.find((s) => s.status === "playing") ??
    sessions.find((s) => s.isCurrent) ??
    sessions[0] ??
    null
  );
}
