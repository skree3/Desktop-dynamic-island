import { useEffect, useState } from "react";
import { Music2, Pause, Play, SkipBack, SkipForward, Volume1, Volume2, VolumeX } from "lucide-react";
import { IconButton, Note, Slider } from "../components/ui";
import { useLoad, useNow } from "../hooks/common";
import { errorMessage, ipc } from "../services/ipc";
import { useApp } from "../stores/app";
import { currentSession, useMedia } from "../stores/media";
import type { MediaAction, MediaSession } from "../types";
import { formatClock, mediaPosition } from "../utils/format";

export default function MusicCard() {
  const sessions = useMedia((s) => s.sessions);
  const selectedId = useMedia((s) => s.selectedId);
  const artwork = useMedia((s) => s.artwork);
  const showArt = useApp((s) => s.settings?.music.showArtwork ?? true);
  const volumeModule = useApp((s) => s.settings?.modules.volume ?? false);
  const [error, setError] = useState<string | null>(null);
  const s = currentSession(sessions, selectedId);
  const now = useNow(500, s?.status === "playing");

  useEffect(() => {
    ipc.mediaSessions().then((list) => useMedia.getState().setSessions(list)).catch((e) => setError(errorMessage(e)));
  }, []);
  useEffect(() => {
    if (s && showArt) useMedia.getState().loadArtwork(s);
  }, [s?.artworkKey, showArt]);

  const control = async (action: MediaAction) => {
    if (!s) return;
    setError(null);
    try {
      await ipc.mediaControl(s.id, action);
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  if (!s) {
    return (
      <div className="card-pad">
        <div className="empty">
          <Music2 size={28} />
          <span>Нет активных медиасессий</span>
        </div>
        {error && <Note kind="error">{error}</Note>}
      </div>
    );
  }

  const art = artwork[s.artworkKey];
  const playing = s.status === "playing";
  const pos = mediaPosition(s.positionMs, s.positionUpdatedAt, playing, s.durationMs, now);

  return (
    <div className="card-pad music">
      <div className="music-main">
        <div className="music-art">{showArt && art ? <img src={art} alt="" draggable={false} /> : <Music2 size={30} />}</div>
        <div className="music-info">
          <div className="music-title" title={s.title}>{s.title || "Без названия"}</div>
          <div className="music-artist" title={s.artist}>{s.artist || s.album || "—"}</div>
          <div className="music-app">{s.appName}</div>
        </div>
      </div>

      {s.durationMs ? (
        <div className="music-progress">
          <span className="mono">{formatClock(pos)}</span>
          <Slider
            label="Позиция"
            value={pos / 1000}
            max={s.durationMs / 1000}
            disabled={!s.canSeek}
            onCommit={(v) => control({ type: "seek", positionMs: Math.round(v * 1000) })}
          />
          <span className="mono">{formatClock(s.durationMs)}</span>
        </div>
      ) : (
        <div className="music-progress muted small">Приложение не сообщает длительность трека</div>
      )}

      <div className="music-controls">
        <IconButton size="lg" title="Предыдущий трек" disabled={!s.canPrev} onClick={() => control({ type: "previous" })}>
          <SkipBack size={20} />
        </IconButton>
        <button type="button" className="play-btn" disabled={!s.canPlayPause} aria-label={playing ? "Пауза" : "Воспроизвести"} onClick={() => control({ type: "playPause" })}>
          {playing ? <Pause size={22} /> : <Play size={22} />}
        </button>
        <IconButton size="lg" title="Следующий трек" disabled={!s.canNext} onClick={() => control({ type: "next" })}>
          <SkipForward size={20} />
        </IconButton>
      </div>

      <VolumeRow session={s} masterAvailable={volumeModule} onError={setError} />

      {sessions.length > 1 && (
        <div className="session-list">
          {sessions.map((x) => (
            <button key={x.id} type="button" className={`chip ${x.id === s.id ? "on" : ""}`} onClick={() => useMedia.getState().select(x.id)} title={x.title}>
              {x.status === "playing" ? "▶ " : ""}
              {x.appName}
            </button>
          ))}
        </div>
      )}
      {error && <Note kind="error">{error}</Note>}
    </div>
  );
}

function VolumeRow({ session, masterAvailable, onError }: { session: MediaSession; masterAvailable: boolean; onError: (e: string) => void }) {
  // Громкость плеера (MPRIS), иначе — общая громкость системы.
  const perPlayer = session.supportsVolume && session.volume != null;
  const master = useLoad(() => (masterAvailable && !perPlayer ? ipc.volume() : Promise.resolve(null)), [masterAvailable, perPlayer]);
  if (!perPlayer && (!masterAvailable || !master.data)) return null;
  const value = perPlayer ? session.volume! : master.data!.level;
  const muted = !perPlayer && master.data!.muted;
  const Icon = muted || value === 0 ? VolumeX : value < 0.5 ? Volume1 : Volume2;
  return (
    <div className="volume-row">
      <IconButton
        size="sm"
        title={perPlayer ? "Громкость плеера" : muted ? "Включить звук" : "Выключить звук"}
        onClick={() => {
          if (!perPlayer) ipc.volumeMute(!muted).then(master.reload).catch((e) => onError(errorMessage(e)));
        }}
      >
        <Icon size={15} />
      </IconButton>
      <Slider
        label="Громкость"
        value={Math.round(value * 100)}
        onCommit={(v) => {
          const p = perPlayer ? ipc.mediaControl(session.id, { type: "setVolume", volume: v / 100 }) : ipc.volumeSet(v / 100);
          p.catch((e) => onError(errorMessage(e)));
        }}
      />
      <span className="small muted">{perPlayer ? "плеер" : "система"}</span>
    </div>
  );
}
