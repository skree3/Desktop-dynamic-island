// Хост карточки плагина: изолированный iframe (sandbox="allow-scripts", без
// allow-same-origin) + мост postMessage → plugin_call. Права проверяет Rust.

import { useEffect, useRef, useState } from "react";
import { Note, Spinner } from "../components/ui";
import { useLoad } from "../hooks/common";
import { events, ipc, on, pluginUrl } from "../services/ipc";
import type { CardProps } from ".";

const frames = new Map<Window, { pluginId: string; setHeight: (h: number) => void }>();
let bridgeInstalled = false;

function installBridge() {
  if (bridgeInstalled) return;
  bridgeInstalled = true;
  window.addEventListener("message", async (ev: MessageEvent) => {
    const src = ev.source as Window | null;
    if (!src) return;
    const entry = frames.get(src);
    if (!entry) return; // сообщения только от зарегистрированных iframe плагинов
    const d = ev.data;
    if (!d || typeof d !== "object") return;
    if (d.__island_size) {
      if (typeof d.height === "number") entry.setHeight(d.height);
      return;
    }
    if (!d.__island || typeof d.method !== "string" || typeof d.id !== "number") return;
    try {
      const result = await ipc.pluginCall(entry.pluginId, d.method, d.params ?? {});
      src.postMessage({ __island: 1, id: d.id, result }, "*");
    } catch (e: any) {
      src.postMessage({ __island: 1, id: d.id, error: { kind: e?.kind ?? "other", message: e?.message ?? String(e) } }, "*");
    }
  });
  on<{ pluginId: string; event: string; data: unknown }>(events.PLUGINS_EVENT, (p) => {
    for (const [win, f] of frames) if (f.pluginId === p.pluginId) win.postMessage({ __island_event: p.event, data: p.data }, "*");
  });
  on<{ pluginId: string }>("plugins://settings", (p) => {
    for (const [win, f] of frames) if (f.pluginId === p.pluginId) win.postMessage({ __island_event: "settingsChanged", data: null }, "*");
  });
}

export default function PluginCard({ pluginId }: CardProps) {
  const plugins = useLoad(() => ipc.plugins());
  const frame = useRef<HTMLIFrameElement>(null);
  const [height, setHeight] = useState(160);
  const plugin = plugins.data?.find((p) => p.manifest.id === pluginId);
  const maxH = plugin?.manifest.card?.height ?? 160;

  useEffect(() => {
    installBridge();
  }, []);

  useEffect(() => {
    const win = frame.current?.contentWindow;
    if (!win || !plugin) return;
    frames.set(win, { pluginId: plugin.manifest.id, setHeight: (h) => setHeight(Math.max(60, Math.min(maxH, h))) });
    return () => {
      frames.delete(win);
    };
  }, [plugin, maxH]);

  if (!plugins.data) return <div className="card-pad center"><Spinner /></div>;
  if (!plugin) return <div className="card-pad"><Note kind="warn">Плагин не найден</Note></div>;
  if (!plugin.enabled) return <div className="card-pad"><Note kind="warn">Плагин отключён</Note></div>;
  return (
    <div className="plugin-card">
      <iframe
        ref={frame}
        title={plugin.manifest.name}
        src={pluginUrl(plugin.manifest.id, plugin.manifest.entry)}
        sandbox="allow-scripts"
        referrerPolicy="no-referrer"
        style={{ height: Math.max(60, Math.min(maxH, height)) }}
      />
    </div>
  );
}
