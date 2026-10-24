import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, Entry, Stats } from "../api";
import type { PageProps } from "../App";

const PAGE = 50;

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

  const saved = stats ? stats.minutes_saved : 0;
  const savedLabel = saved >= 60 ? `${Math.floor(saved / 60)}h ${Math.round(saved % 60)}m` : `${Math.round(saved)}m`;

  return (
    <>
      <h1>Home</h1>
      {!settings.model && (
        <div className="banner">No speech model yet. Go to <b>Models</b> and download one to start.</div>
      )}
      {stats && (
        <div className="stats">
          <div><b>{stats.words_today.toLocaleString()}</b><span>words today</span></div>
          <div><b>{stats.words_week.toLocaleString()}</b><span>this week</span></div>
          <div><b>{stats.total_words.toLocaleString()}</b><span>all time</span></div>
          <div><b>{savedLabel}</b><span>saved vs typing</span></div>
          <div><b>{stats.streak_days}</b><span>day streak</span></div>
        </div>
      )}
      <div className="row between">
        <h2>History</h2>
        <div className="row">
          <input placeholder="Search" value={query} onChange={(e) => setQuery(e.target.value)} />
          <button onClick={() => api.repasteLast()}>Paste last again</button>
          {rows.length > 0 && (
            <button className="ghost" onClick={() => api.historyClear().then(load)}>Clear all</button>
          )}
        </div>
      </div>
      {rows.length === 0 && <p className="muted">Nothing yet. Hold your shortcut in any app and say something.</p>}
      <ul className="history">
        {rows.map((e) => (
          <li key={e.id}>
            <p>{e.text}</p>
            <div className="meta">
              <span>{new Date(e.ts * 1000).toLocaleString()}</span>
              {e.app && <span>{e.app}</span>}
              <span>{e.words} words</span>
              <button className="ghost sm" onClick={() => copy(e)}>{copied === e.id ? "Copied" : "Copy"}</button>
              <button className="ghost sm" onClick={() => api.historyDelete(e.id).then(load)}>Delete</button>
            </div>
          </li>
        ))}
      </ul>
    </>
  );
}
