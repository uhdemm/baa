import { useState } from "react";
import { AppProfile, AutoSend } from "../api";
import type { PageProps } from "../App";

const SEND: [string, string][] = [["", "Default"], ["off", "Don't send"], ["enter", "Press Enter"], ["ctrl_enter", "Press Ctrl+Enter"]];
const STYLE: [string, string][] = [["", "As spoken"], ["no_period", "No final period"], ["lowercase", "all lowercase, casual"]];
const SUGGEST = ["discord.exe", "slack.exe", "chrome.exe", "code.exe", "windowsterminal.exe", "outlook.exe", "claude.exe"];

export default function Apps({ settings, save }: PageProps) {
  const [exe, setExe] = useState("");
  const profiles = settings.app_profiles;
  const update = (i: number, patch: Partial<AppProfile>) =>
    save({ ...settings, app_profiles: profiles.map((p, k) => (k === i ? { ...p, ...patch } : p)) });
  const add = (name: string) => {
    const n = name.trim().toLowerCase();
    if (!n || profiles.some((p) => p.exe === n)) return;
    save({ ...settings, app_profiles: [...profiles, { exe: n.endsWith(".exe") ? n : `${n}.exe`, auto_send: null, style: null }] });
    setExe("");
  };

  return (
    <>
      <h1>Apps</h1>
      <p className="muted">Different behavior per app. Example: press Enter after dictating in Discord, all lowercase in chats.</p>
      <div className="row">
        <input value={exe} placeholder="app name, e.g. discord.exe" onChange={(e) => setExe(e.target.value)} onKeyDown={(e) => e.key === "Enter" && add(exe)} />
        <button onClick={() => add(exe)}>Add app</button>
      </div>
      <div className="chips">
        {SUGGEST.filter((s) => !profiles.some((p) => p.exe === s)).map((s) => (
          <button key={s} className="chip add" onClick={() => add(s)}>+ {s}</button>
        ))}
      </div>
      {profiles.length > 0 && (
        <table className="pairs">
          <thead><tr><th>App</th><th>After pasting</th><th>Style</th><th /></tr></thead>
          <tbody>
            {profiles.map((p, i) => (
              <tr key={p.exe}>
                <td><b>{p.exe}</b></td>
                <td>
                  <select value={p.auto_send ?? ""} onChange={(e) => update(i, { auto_send: (e.target.value || null) as AutoSend | null })}>
                    {SEND.map(([v, l]) => <option key={v} value={v}>{l}</option>)}
                  </select>
                </td>
                <td>
                  <select value={p.style ?? ""} onChange={(e) => update(i, { style: e.target.value || null })}>
                    {STYLE.map(([v, l]) => <option key={v} value={v}>{l}</option>)}
                  </select>
                </td>
                <td><button className="ghost sm" onClick={() => save({ ...settings, app_profiles: profiles.filter((_, k) => k !== i) })}>Remove</button></td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </>
  );
}
