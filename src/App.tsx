import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api, Settings } from "./api";
import { applyAppearance } from "./appearance";
import Face from "./ui/Face";
import { Icon, Keys } from "./ui/controls";
import Home from "./pages/Home";
import Models from "./pages/Models";
import Words from "./pages/Words";
import Apps from "./pages/Apps";
import Prefs from "./pages/Prefs";
import Ai from "./pages/Ai";
import Look from "./pages/Look";
import Setup from "./pages/Setup";
import Notes from "./pages/Notes";
import Style from "./pages/Style";
import Buddy from "./pages/Buddy";

const PAGES = [
  { id: "home", label: "Home", icon: "home", group: "Baa" },
  { id: "notes", label: "Notes", icon: "notes", group: "Baa" },
  { id: "buddy", label: "Buddy", icon: "buddy", group: "Make it yours" },
  { id: "style", label: "Style", icon: "style", group: "Make it yours" },
  { id: "words", label: "Words", icon: "words", group: "Make it yours" },
  { id: "apps", label: "Apps", icon: "apps", group: "Make it yours" },
  { id: "models", label: "Models", icon: "models", group: "Settings" },
  { id: "ai", label: "AI", icon: "ai", group: "Settings" },
  { id: "look", label: "Appearance", icon: "look", group: "Settings" },
  { id: "settings", label: "General", icon: "settings", group: "Settings" },
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

const inTauri = document.documentElement.classList.contains("tauri"); // set in main.tsx before the dev mock

/** The red / yellow / green window buttons. Close hides to the tray (Baa keeps running). */
function Lights() {
  const w = () => getCurrentWindow();
  return (
    <div className="lights">
      <button className="close" aria-label="Close" onClick={() => inTauri && w().close()} />
      <button className="min" aria-label="Minimize" onClick={() => inTauri && w().minimize()} />
      <button className="max" aria-label="Zoom" onClick={() => inTauri && w().toggleMaximize()} />
    </div>
  );
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

  useEffect(() => { if (settings?.appearance) applyAppearance(settings.appearance); }, [settings?.appearance]);

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
  const current = PAGES.find((p) => p.id === page)!;

  return (
    <div className="shell">
      <nav className="side glass">
        <div className="titlebar" data-tauri-drag-region>
          <Lights />
          <div className="drag" data-tauri-drag-region />
        </div>
        <div className="brand"><Face size={18} /><b>Baa</b></div>
        {(["Baa", "Make it yours", "Settings"] as const).map((g) => (
          <div key={g}>
            {g !== "Baa" && <div className="nav-head">{g}</div>}
            {PAGES.filter((p) => p.group === g).map((p) => (
              <button key={p.id} className={`nav ${page === p.id ? "on" : ""}`} onClick={() => setPage(p.id)}>
                <Icon name={p.icon} size={16} />{p.label}
              </button>
            ))}
          </div>
        ))}
        <div className="side-foot">
          <span>Hold to dictate</span>
          <Keys combo={settings.shortcut} />
        </div>
      </nav>
      <div className="main">
        <header className="toolbar" data-tauri-drag-region>
          <h1 data-tauri-drag-region>{current.label}</h1>
          <div className="spacer" data-tauri-drag-region />
        </header>
        <main className="page">
          <div className="page-inner">
            {error && <div className="banner err" onClick={() => setError(null)}>{error}</div>}
            {page === "home" && <Home {...props} go={(p) => setPage(p as PageId)} />}
            {page === "notes" && <Notes {...props} />}
            {page === "style" && <Style {...props} />}
            {page === "buddy" && <Buddy {...props} />}
            {page === "models" && <Models {...props} />}
            {page === "words" && <Words {...props} />}
            {page === "apps" && <Apps {...props} />}
            {page === "ai" && <Ai {...props} />}
            {page === "look" && <Look {...props} />}
            {page === "settings" && <Prefs {...props} />}
          </div>
        </main>
      </div>
    </div>
  );
}
