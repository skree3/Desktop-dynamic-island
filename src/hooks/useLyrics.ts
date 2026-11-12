// Текст песни для текущего трека (LRCLIB через бэкенд) и текущая строка по позиции.

import { useEffect, useState } from "react";
import { ipc } from "../services/ipc";
import { useApp } from "../stores/app";
import type { Lyrics, MediaSession } from "../types";

const cache = new Map<string, Lyrics | null>();

function trackKey(s: MediaSession): string {
  return `${s.artist}\u0001${s.title}\u0001${Math.round((s.durationMs ?? 0) / 2000)}`;
}

/** Строка, которая звучит в момент `posMs` (null — до первой строки или пауза в тексте). */
export function currentLine(lines: [number, string][], posMs: number): string | null {
  let lo = 0;
  let hi = lines.length - 1;
  let found = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (lines[mid][0] <= posMs) {
      found = mid;
      lo = mid + 1;
    } else hi = mid - 1;
  }
  if (found < 0) return null;
  const text = lines[found][1].trim();
  return text || null;
}

export function useLyrics(s: MediaSession | null): Lyrics | null {
  const enabled = useApp((st) => !!st.settings?.music.lyrics);
  const key = s && enabled && s.title && s.artist ? trackKey(s) : null;
  const [lyrics, setLyrics] = useState<Lyrics | null>(key ? (cache.get(key) ?? null) : null);
  useEffect(() => {
    if (!key || !s) {
      setLyrics(null);
      return;
    }
    if (cache.has(key)) {
      setLyrics(cache.get(key) ?? null);
      return;
    }
    let alive = true;
    setLyrics(null);
    ipc
      .lyrics(s.artist, s.title, s.album, s.durationMs)
      .then((l) => {
        cache.set(key, l && l.lines.length ? l : null);
        if (alive) setLyrics(cache.get(key) ?? null);
      })
      .catch(() => {
        // Сеть недоступна — попробуем снова при следующем треке.
      });
    return () => {
      alive = false;
    };
  }, [key]);
  return lyrics;
}
