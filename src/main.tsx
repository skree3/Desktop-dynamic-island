// Точка входа фронтенда. Одно приложение Vite обслуживает два окна,
// но код окна настроек загружается отдельным чанком только в своём окне.

import { lazy, StrictMode, Suspense } from "react";
import { createRoot } from "react-dom/client";
import "./styles/base.css";

const params = new URLSearchParams(window.location.search);
const windowKind = params.get("window") ?? "island";

const IslandApp = lazy(() => import("./pages/IslandApp"));
const SettingsApp = lazy(() => import("./pages/SettingsApp"));

document.documentElement.dataset.window = windowKind;

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Suspense fallback={null}>{windowKind === "settings" ? <SettingsApp initialSection={params.get("section")} /> : <IslandApp />}</Suspense>
  </StrictMode>,
);
