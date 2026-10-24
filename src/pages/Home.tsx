import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, Entry, Stats } from "../api";
import type { PageProps } from "../App";
import Face from "../ui/Face";
import Wave from "../ui/Wave";
import { Icon } from "../ui/controls";

const PAGE = 50;

function ago(ts: number) {
  const s = Date.now() / 1000 - ts;
  if (s < 60) return "just now";
  if (s < 3600) return `${Math.floor(s / 60)} min ago`;
  if (s < 86400) return `${Math.floor(s / 3600)} h ago`;
  return new Date(ts * 1000).toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

export default function Home({ settings }: PageProps) {
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
  const saved = stats?.minutes_saved ?? 0;
  const savedLabel = saved >= 60 ? `${Math.floor(saved / 60)}h ${Math.round(saved % 60)}m` : `${Math.round(saved)}m`;

  return (
    <>
      <div className="page-head">
        <h1>Ready when you are.</h1>
        <p>Hold {settings.shortcut} to start dictating from anywhere.</p>
      </div>
      {!settings.model && <div className="banner warn">No speech model yet. Open Models and download one to start.</div>}
      <div className="hero-bar">
        <div className="dbar">
          <div className="face-wrap"><Face size={33} /></div>
          <Wave style="idle" />
          <div className="divider" />
          <span className="label">Press {settings.shortcut.replace("+", " ")} to dictate...</span>
          <i className="caret" />
        </div>
      </div>

      {stats && (
        <div className="features">
          <div className="feature"><div className="fi"><Icon name="fast" /></div><b>{stats.words_today.toLocaleString()}</b><span>words today</span></div>
          <div className="feature"><div className="fi"><Icon name="words" /></div><b>{stats.total_words.toLocaleString()}</b><span>words all time</span></div>
          <div className="feature"><div className="fi"><Icon name="history" /></div><b>{savedLabel}</b><span>saved vs typing</span></div>
          <div className="feature"><div className="fi"><Icon name="check" /></div><b>{stats.streak_days}</b><span>day streak</span></div>
        </div>
      )}

      <div className="section">
        <h2>History</h2>
        <p className="lead">Everything you've dictated, newest first. Stays on this PC.</p>
        <div className="searchbar">
          <input className="input" style={{ flex: 1, maxWidth: 360 }} placeholder="Search" value={query} onChange={(e) => setQuery(e.target.value)} />
          <button className="btn" onClick={() => api.repasteLast()}>Paste last again</button>
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
