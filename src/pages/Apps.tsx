import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AppProfile, AutoSend } from "../api";
import type { PageProps } from "../App";
import { Row, Select } from "../ui/controls";

const SEND = [{ value: "", label: "Like everywhere else" }, { value: "off", label: "Just paste" }, { value: "enter", label: "Paste + press Enter" }, { value: "ctrl_enter", label: "Paste + Ctrl+Enter" }];
const STYLE = [{ value: "", label: "Normal" }, { value: "no_period", label: "No final period" }, { value: "lowercase", label: "all lowercase" }];
const POPULAR = ["discord.exe", "slack.exe", "chrome.exe", "code.exe", "windowsterminal.exe", "outlook.exe", "claude.exe"];

/** "windowsterminal.exe" -> "Windowsterminal": readable enough without a lookup table. */
const pretty = (exe: string) => { const n = exe.replace(/\.exe$/i, ""); return n.charAt(0).toUpperCase() + n.slice(1); };

export default function Apps({ settings, save }: PageProps) {
  const [exe, setExe] = useState("");
  const [open, setOpen] = useState<string[]>([]);
  const profiles = settings.app_profiles;
  useEffect(() => { invoke<string[]>("open_apps").then(setOpen).catch(() => {}); }, []);
  const update = (i: number, patch: Partial<AppProfile>) => save({ ...settings, app_profiles: profiles.map((p, k) => (k === i ? { ...p, ...patch } : p)) });
  const add = (name: string) => {
    const n = name.trim().toLowerCase();
    if (!n) return;
    const exeName = n.endsWith(".exe") ? n : `${n}.exe`;
    if (profiles.some((p) => p.exe === exeName)) return;
    save({ ...settings, app_profiles: [...profiles, { exe: exeName, auto_send: null, style: null }] });
    setExe("");
  };
  const has = (e: string) => profiles.some((p) => p.exe === e);
  const openNow = open.filter((e) => !has(e));
  const popular = POPULAR.filter((e) => !has(e) && !open.includes(e));

  return (
    <>
      <div className="page-head">
        <h1>Apps</h1>
        <p>Make Baa act differently in certain apps: press Enter for you in Discord so messages send, write all lowercase in chats, and so on.</p>
      </div>
      {openNow.length > 0 && (
        <div className="section">
          <h2>Open right now</h2>
          <div className="chips">{openNow.map((e) => <button key={e} className="chip add" onClick={() => add(e)}>+ {pretty(e)}</button>)}</div>
        </div>
      )}
      {popular.length > 0 && (
        <div className="section">
          <h2>Popular</h2>
          <div className="chips">{popular.map((e) => <button key={e} className="chip add" onClick={() => add(e)}>+ {pretty(e)}</button>)}</div>
        </div>
      )}
      <div className="searchbar">
        <input className="input" style={{ width: 280 }} value={exe} placeholder="Or type the app's program name" onChange={(e) => setExe(e.target.value)} onKeyDown={(e) => e.key === "Enter" && add(exe)} />
        <button className="btn" onClick={() => add(exe)}>Add</button>
      </div>
      {profiles.length > 0 ? (
        <div className="section">
          <h2>Your apps</h2>
          <div className="rows">
            {profiles.map((p, i) => (
              <Row key={p.exe} title={pretty(p.exe)} desc="After pasting · how it writes">
                <Select width={170} value={p.auto_send ?? ""} options={SEND} onChange={(v) => update(i, { auto_send: (v || null) as AutoSend | null })} />
                <Select width={140} value={p.style ?? ""} options={STYLE} onChange={(v) => update(i, { style: v || null })} />
                <button className="btn ghost sm" onClick={() => save({ ...settings, app_profiles: profiles.filter((_, k) => k !== i) })}>Remove</button>
              </Row>
            ))}
          </div>
        </div>
      ) : (
        <div className="empty">No apps yet. Everything works the same in every app until you add one.</div>
      )}
    </>
  );
}
