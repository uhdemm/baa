import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, Settings } from "./api";
import Home from "./pages/Home";
import Models from "./pages/Models";
import Words from "./pages/Words";
import Apps from "./pages/Apps";
import Prefs from "./pages/Prefs";
import Ai from "./pages/Ai";
import Setup from "./pages/Setup";

const PAGES = [
  { id: "home", label: "Home" },
  { id: "models", label: "Models" },
  { id: "words", label: "Words" },
  { id: "apps", label: "Apps" },
  { id: "ai", label: "AI" },
  { id: "settings", label: "Settings" },
] as const;
type PageId = (typeof PAGES)[number]["id"];

const fromHash = (): PageId => {
  const h = location.hash.replace("#", "");
  return (PAGES.find((p) => p.id === h)?.id ?? "home") as PageId;
};

export default function App() {
  const [page, setPage] = useState<PageId>(fromHash);
  const [settings, setSettingsState] = useState<Settings | null>(null);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(() => api.getSettings().then(setSettingsState), []);

  useEffect(() => {
    reload();
    const un1 = listen<string>("navigate", (e) => setPage(e.payload as PageId));
    const un2 = listen("settings-changed", () => reload());
    return () => {
      un1.then((f) => f());
      un2.then((f) => f());
    };
  }, [reload]);

  // Every page edits settings through this: saves, and shows the error if the backend refuses.
  const save = useCallback(async (next: Settings) => {
    setSettingsState(next);
    try {
      setSettingsState(await api.setSettings(next));
      setError(null);
    } catch (e) {
      setError(String(e));
      reload();
    }
  }, [reload]);

  if (!settings) return null;
  const props = { settings, save };
  if (!settings.onboarded) return <Setup {...props} />;

  return (
    <div className="shell">
      <nav className="side">
        <div className="brand">Baa</div>
        {PAGES.map((p) => (
          <button key={p.id} className={page === p.id ? "on" : ""} onClick={() => setPage(p.id)}>
            {p.label}
          </button>
        ))}
        <div className="hint">
          Hold <kbd>{settings.shortcut}</kbd> and talk
        </div>
      </nav>
      <main className="page">
        {error && <div className="banner err" onClick={() => setError(null)}>{error}</div>}
        {page === "home" && <Home {...props} />}
        {page === "models" && <Models {...props} />}
        {page === "words" && <Words {...props} />}
        {page === "apps" && <Apps {...props} />}
        {page === "ai" && <Ai {...props} />}
        {page === "settings" && <Prefs {...props} />}
      </main>
    </div>
  );
}

export interface PageProps {
  settings: Settings;
  save: (s: Settings) => Promise<void>;
}
