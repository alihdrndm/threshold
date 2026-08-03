import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { resolveWindow } from "./windows/registry";
import { isTauri } from "./lib/tauri";
import "./styles/index.css";

// Outside Tauri (plain `npm run dev` in a browser) there is no window label,
// so fall back to ?window= for frontend-only iteration.
function currentLabel(): string {
  if (isTauri()) return getCurrentWebviewWindow().label;
  return new URLSearchParams(window.location.search).get("window") ?? "main";
}

const Window = resolveWindow(currentLabel());

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Window />
  </React.StrictMode>,
);
