import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles.css";

// inside the real app the window itself is Mica glass, so the page lets it show through
if ((window as unknown as Record<string, unknown>).__TAURI_INTERNALS__) document.documentElement.classList.add("tauri");

if (import.meta.env.DEV) (await import("./devmock")).installDevMock();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
