// Поиск и запуск установленных приложений.

import { useEffect, useRef, useState } from "react";
import { Globe, Plus, RefreshCw, Search, Star } from "lucide-react";
import { AppIcon, IconButton, Note, Spinner } from "../components/ui";
import { errorMessage, ipc } from "../services/ipc";
import { useIsland } from "../stores/island";
import type { AppEntry } from "../types";

export default function AppsCard() {
  const [query, setQuery] = useState("");
  const [items, setItems] = useState<AppEntry[] | null>(null);
  const [sel, setSel] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  useEffect(() => {
    let alive = true;
    const t = window.setTimeout(() => {
      ipc
        .appsSearch(query, 30)
        .then((r) => {
          if (alive) {
            setItems(r);
            setSel(0);
            setError(null);
          }
        })
        .catch((e) => alive && setError(errorMessage(e)));
    }, 90);
    return () => {
      alive = false;
      window.clearTimeout(t);
    };
  }, [query]);

  const launch = async (a: AppEntry) => {
    try {
      await ipc.appsLaunch(a.id);
      useIsland.getState().collapse();
    } catch (e) {
      setError(errorMessage(e));
    }
  };

  const toggleFav = async (a: AppEntry) => {
    await ipc.appsSetFavorite(a.id, !a.favorite);
    setItems((list) => list?.map((x) => (x.id === a.id ? { ...x, favorite: !x.favorite } : x)) ?? null);
  };

  return (
    <div className="card-pad">
      <div className="search">
        <Search size={14} />
        <input
          ref={inputRef}
          className="input-bare"
          placeholder="Найти приложение"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (!items?.length) return;
            if (e.key === "ArrowDown") setSel((s) => Math.min(items.length - 1, s + 1));
            if (e.key === "ArrowUp") setSel((s) => Math.max(0, s - 1));
            if (e.key === "Enter") launch(items[sel]);
          }}
        />
        <IconButton
          size="sm"
          title="Обновить список программ"
          onClick={async () => {
            setRefreshing(true);
            try {
              await ipc.appsRefresh();
              setItems(await ipc.appsSearch(query, 30));
            } catch (e) {
              setError(errorMessage(e));
            } finally {
              setRefreshing(false);
            }
          }}
        >
          {refreshing ? <Spinner size={13} /> : <RefreshCw size={13} />}
        </IconButton>
        <IconButton size="sm" title="Добавить приложение вручную" onClick={() => ipc.openSettings("integrations")}>
          <Plus size={14} />
        </IconButton>
      </div>
      {error && <Note kind="error">{error}</Note>}
      {!items ? (
        <div className="center"><Spinner /></div>
      ) : items.length === 0 ? (
        <div className="empty small">Ничего не найдено</div>
      ) : (
        <div className="apps-grid scroll" style={{ maxHeight: 330 }}>
          {items.map((a, i) => (
            <div key={a.id} className={`app-item ${i === sel ? "sel" : ""}`} onClick={() => launch(a)} onMouseEnter={() => setSel(i)} title={a.exePath ?? a.name}>
              <AppIcon id={a.id} size={30} fallback={a.source === "web" ? <Globe size={18} /> : undefined} />
              <span className="app-name">{a.name}</span>
              <button
                type="button"
                className={`fav ${a.favorite ? "on" : ""}`}
                aria-label={a.favorite ? "Убрать из избранного" : "В избранное"}
                onClick={(e) => {
                  e.stopPropagation();
                  toggleFav(a);
                }}
              >
                <Star size={12} />
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
