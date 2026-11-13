// Текст песни на весь остров: текущая строка крупно по центру, соседние —
// приглушённо; список плавно сдвигается вслед за песней.

import { useEffect, useRef, useState } from "react";
import { useNow } from "../hooks/common";
import { lineIndex, useLyrics } from "../hooks/useLyrics";
import { ipc } from "../services/ipc";
import { useIsland } from "../stores/island";
import { currentSession, useMedia } from "../stores/media";
import { mediaPosition } from "../utils/format";

const HEIGHT = 280;

export function LyricsView() {
  const sessions = useMedia((s) => s.sessions);
  const selected = useMedia((s) => s.selectedId);
  const artwork = useMedia((s) => s.artwork);
  const s = currentSession(sessions, selected);
  const lyrics = useLyrics(s);
  const playing = s?.status === "playing";
  const now = useNow(200, !!lyrics && playing);
  const listRef = useRef<HTMLDivElement>(null);
  const [offset, setOffset] = useState(0);

  const idx = s && lyrics ? lineIndex(lyrics.lines, mediaPosition(s.positionMs, s.positionUpdatedAt, playing, s.durationMs, now) + 250) : -1;

  // Текущая строка — по вертикальному центру панели.
  useEffect(() => {
    const list = listRef.current;
    if (!list) return;
    const el = list.children[Math.max(0, idx)] as HTMLElement | undefined;
    if (!el) return;
    setOffset(HEIGHT / 2 - (el.offsetTop + el.offsetHeight / 2));
  }, [idx, lyrics]);

  if (!s) return <div className="card-pad muted center" style={{ height: 120 }}>Сейчас ничего не играет</div>;
  const art = artwork[s.artworkKey];

  return (
    <div className="lyrics-full" style={{ height: HEIGHT }}>
      {art && <img className="lyrics-bg" src={art} alt="" draggable={false} />}
      <div className="lyrics-head">
        <span className="lyrics-track">{s.title}</span>
        <span className="lyrics-artist">{s.artist}</span>
      </div>
      {!lyrics ? (
        <div className="lyrics-empty">Синхронный текст для этого трека не найден</div>
      ) : (
        <div className="lyrics-list" ref={listRef} style={{ transform: `translateY(${offset}px)` }}>
          {lyrics.lines.map(([t, text], i) => (
            <button
              key={`${t}-${i}`}
              type="button"
              className={`lyrics-line ${i === idx ? "cur" : i < idx ? "past" : ""}`}
              onClick={() => (i === idx ? useIsland.getState().setCard("music") : ipc.mediaControl(s.id, { type: "seek", positionMs: t }).catch(() => {}))}
            >
              {text.trim() || " "}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
