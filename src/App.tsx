import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, Settings } from "./api";
import Face from "./ui/Face";
import { Icon, Keys } from "./ui/controls";
import Home from "./pages/Home";
import Models from "./pages/Models";
import Words from "./pages/Words";
import Apps from "./pages/Apps";
import Prefs from "./pages/Prefs";
import Ai from "./pages/Ai";
import Setup from "./pages/Setup";

const PAGES = [
  { id: "home", label: "Home", icon: "home" },
  { id: "models", label: "Models", icon: "models" },
  { id: "words", label: "Words", icon: "words" },
  { id: "apps", label: "Apps", icon: "apps" },
  { id: "ai", label: "AI", icon: "ai" },
  { id: "settings", label: "Settings", icon: "settings" },
] as const;
type PageId = (typeof PAGES)[number]["id"];

const fromHash = (): PageId => {
  const h = location.hash.replace("#", "");
  return (PAGES.find((p) => p.id === h)?.id ?? "home") as PageId;
};

export interface PageProps {
  settings: Settings;
  save: (s: Settings) => void;
}

export default function App() {
  const [page, setPage] = useState<PageId>(fromHash);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [error, setError] = useState<string | null>(null);
  const pending = useRef<number | undefined>(undefined);
  const latest = useRef<Settings | null>(null);

  const reload = useCallback(() => api.getSettings().then((s) => { latest.current = s; setSettings(s); }), []);

  useEffect(() => {
    reload();
    api.startupWarning().then((w) => w && setError(w));
    const un1 = listen<string>("navigate", (e) => setPage(e.payload as PageId));
    // the backend changed settings (model downloaded, word learned): take its copy unless we're mid-edit
    const un2 = listen("settings-changed", () => { if (!pending.current) reload(); });
    return () => { un1.then((f) => f()); un2.then((f) => f()); };
  }, [reload]);

  // Edits show instantly; the write happens once typing pauses, and the backend's
  // reply never overwrites newer keystrokes.
  const save = useCallback((next: Settings) => {
    latest.current = next;
    setSettings(next);
    window.clearTimeout(pending.current);
    pending.current = window.setTimeout(async () => {
      pending.current = undefined;
      try {
        await api.setSettings(latest.current!);
        setError(null);
      } catch (e) {
        setError(String(e));
        reload();
      }
    }, 250);
  }, [reload]);

  if (!settings) return null;
  const props = { settings, save };
  if (!settings.onboarded) return <Setup {...props} />;

  return (
    <div className="shell">
      <nav className="side">
        <div className="brand"><Face size={22} /><b>Baa</b></div>
        {PAGES.map((p) => (
          <button key={p.id} className={`nav ${page === p.id ? "on" : ""}`} onClick={() => setPage(p.id)}>
            <Icon name={p.icon} size={16} />{p.label}
          </button>
        ))}
        <div className="side-foot">
          <span>Hold to dictate</span>
          <Keys combo={settings.shortcut} />
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
