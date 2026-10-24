import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { api, Entry, Stats } from "../api";
import type { PageProps } from "../App";
import Face from "../ui/Face";
import Wave from "../ui/Wave";
import { Keys } from "../ui/controls";

const PAGE = 50;

function ago(ts: number) {
  const s = Date.now() / 1000 - ts;
  if (s < 60) return "just now";
  if (s < 3600) return `${Math.floor(s / 60)} min ago`;
  if (s < 86400) return `${Math.floor(s / 3600)} h ago`;
  return new Date(ts * 1000).toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

const HIDE_KEY = "baa.getStarted.hidden";
const hiddenAtStart = (() => { try { return localStorage.getItem(HIDE_KEY) === "1"; } catch { return false; } })();

/** First-run checklist + the tricks people miss. Goes away once you're set up. */
function GetStarted({ settings, stats, go }: { settings: PageProps["settings"]; stats: Stats | null; go: (page: string) => void }) {
  const [hidden, setHidden] = useState(hiddenAtStart);
  const steps = [
    { done: !!settings.model, title: "Get a speech model", desc: "The part that turns your voice into words. Runs on this PC.", act: "Models", page: "models" },
    { done: (stats?.total_dictations ?? 0) > 0, title: "Say something", desc: `Click into any text box, hold ${settings.shortcut.replace(/\+/g, " ")} and talk. Let go and your words appear.` },
    { done: settings.ai.cleanup, title: "Turn on AI clean-up", desc: "Optional. Fixes grammar, makes lists and emails look right. Free options included.", act: "Set up", page: "ai" },
    { done: settings.overlay_style === "buddy", title: "Get a buddy", desc: "Optional. A little sheep on your taskbar that says your words in a speech bubble.", act: "Pick one", page: "buddy" },
  ];
  if (hidden || steps.every((x) => x.done)) return null;
  const hide = () => { setHidden(true); try { localStorage.setItem(HIDE_KEY, "1"); } catch { /* fine */ } };
  return (
    <div className="section get-started">
      <div className="gs-head"><h2>Get started</h2><button className="btn ghost sm" onClick={hide}>Hide</button></div>
      <ol className="gs-steps">
        {steps.map((st) => (
          <li key={st.title} className={st.done ? "done" : ""}>
            <i className="gs-check">{st.done ? "✓" : ""}</i>
            <div><b>{st.title}</b><span>{st.desc}</span></div>
            {!st.done && st.page && <button className="btn sm" onClick={() => go(st.page!)}>{st.act}</button>}
          </li>
        ))}
      </ol>
      <div className="gs-tricks">
        <span><b>Tap</b> the shortcut instead of holding it to talk hands-free; tap again to finish.</span>
        <span><b>Esc</b> cancels while you're talking.</span>
        <span><b>Select text</b>, hold <Keys combo={settings.command_shortcut} /> and say what to change ("make this shorter").</span>
        <span><b><Keys combo={settings.repaste_shortcut} /></b> pastes your last dictation again.</span>
      </div>
    </div>
  );
}

export default function Home({ settings, go }: PageProps & { go: (page: string) => void }) {
  const [stats, setStats] = useState<Stats | null>(null);
  const [rows, setRows] = useState<Entry[]>([]);
  const [query, setQuery] = useState("");
  const [copied, setCopied] = useState<number | null>(null);

  const load = () => {
    api.stats().then(setStats);
    api.historyList(PAGE, 0, query).then(setRows);
  };
  useEffect(load, [query]);
  useEffect(() => {
    const un = listen("history-changed", load);
    return () => void un.then((f) => f());
  }, [query]);

  const copy = async (e: Entry) => {
    await navigator.clipboard.writeText(e.text);
    setCopied(e.id);
    setTimeout(() => setCopied(null), 1200);
  };
  const exportAll = async () => {
    const path = await saveDialog({
      defaultPath: `baa-history-${new Date().toISOString().slice(0, 10)}.csv`,
      filters: [{ name: "Spreadsheet (CSV)", extensions: ["csv"] }, { name: "Text", extensions: ["txt"] }],
    });
    if (path) {
      const n = await api.historyExport(path);
      setNote(`Saved ${n} dictations`);
      setTimeout(() => setNote(null), 2500);
    }
  };
  const [note, setNote] = useState<string | null>(null);
  const saved = stats?.minutes_saved ?? 0;
  const peak = Math.max(1, ...(stats?.daily ?? []).map(([, w]) => w));
  const savedLabel = saved >= 60 ? `${Math.floor(saved / 60)}h ${Math.round(saved % 60)}m` : `${Math.round(saved)}m`;

  return (
    <>
      {!settings.model && <div className="banner warn">No speech model yet. Open Models and download one to start.</div>}
      <div className="hero">
        <div className="dbar">
          <div className="face-wrap"><Face size={29} /></div>
          <Wave style="idle" />
          <div className="divider" />
          <span className="label">Hold {settings.shortcut.replace("+", " ")} to dictate</span>
          <i className="caret" />
        </div>
        <p>Ready when you are. Your words land wherever you're typing.</p>
      </div>

      <GetStarted settings={settings} stats={stats} go={go} />

      {stats && (
        <div className="stats">
          <div className="stat"><b>{stats.words_today.toLocaleString()}</b><span>Words today</span></div>
          <div className="stat"><b>{stats.total_words.toLocaleString()}</b><span>All time</span></div>
          <div className="stat"><b>{savedLabel}</b><span>Saved vs typing</span></div>
          <div className="stat"><b>{stats.streak_days}</b><span>Day streak</span></div>
        </div>
      )}

      {stats && stats.total_words > 0 && (
        <div className="section insights">
          <div className="chart">
            <div className="chart-head"><h2>Last two weeks</h2><span className="hint">{stats.words_week.toLocaleString()} words this week</span></div>
            <div className="bars">
              {stats.daily.map(([day, w]) => (
                <div key={day} className="bar-col" title={`${new Date(day + "T12:00").toLocaleDateString(undefined, { weekday: "short", month: "short", day: "numeric" })}: ${w} words`}>
                  <i style={{ height: `${Math.max(w ? 6 : 2, (w / peak) * 100)}%` }} className={w ? "" : "zero"} />
                  <span>{new Date(day + "T12:00").toLocaleDateString(undefined, { weekday: "narrow" })}</span>
                </div>
              ))}
            </div>
          </div>
          <div className="side-facts">
            <div className="fact"><b>{Math.round(stats.wpm)}</b><span>words per minute when you talk</span><em>typing is about 40</em></div>
            {stats.top_apps.length > 0 && (
              <div className="fact">
                <span>Where you talk most</span>
                <ul className="top-apps">
                  {stats.top_apps.slice(0, 3).map(([app, w]) => <li key={app}><b>{app.replace(/\.exe$/, "")}</b><span>{w.toLocaleString()}</span></li>)}
                </ul>
              </div>
            )}
          </div>
        </div>
      )}

      <div className="section">
        <h2>History</h2>
        <div className="searchbar">
          <input className="input" style={{ flex: 1, maxWidth: 300 }} placeholder="Search history" value={query} onChange={(e) => setQuery(e.target.value)} />
          <button className="btn" onClick={() => api.repasteLast()}>Paste last again</button>
          {rows.length > 0 && <button className="btn ghost" onClick={exportAll}>{note ?? "Export"}</button>}
          {rows.length > 0 && <button className="btn ghost" onClick={() => api.historyClear().then(load)}>Clear all</button>}
        </div>
        {rows.length === 0 && (
          <div className="empty"><Face size={26} mood="thinking" /> Nothing yet. Hold your shortcut in any app and say something.</div>
        )}
        <ul className="history">
          {rows.map((e) => (
            <li key={e.id}>
              <p>{e.text}</p>
              <div className="meta">
                <span>{ago(e.ts)}</span>
                {e.app && <span>{e.app.replace(/\.exe$/, "")}</span>}
                <span>{e.words} words</span>
                <span className="acts">
                  <button className="btn ghost sm" onClick={() => copy(e)}>{copied === e.id ? "Copied" : "Copy"}</button>
                  <button className="btn ghost sm" onClick={() => api.historyDelete(e.id).then(load)}>Delete</button>
                </span>
              </div>
            </li>
          ))}
        </ul>
      </div>
    </>
  );
}
