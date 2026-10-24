import { useState } from "react";
import { AppProfile, AutoSend } from "../api";
import type { PageProps } from "../App";
import { Row, Select } from "../ui/controls";

const SEND = [{ value: "", label: "Default" }, { value: "off", label: "Just paste" }, { value: "enter", label: "Press Enter" }, { value: "ctrl_enter", label: "Press Ctrl+Enter" }];
const STYLE = [{ value: "", label: "As spoken" }, { value: "no_period", label: "No final period" }, { value: "lowercase", label: "all lowercase" }];
const SUGGEST = ["discord.exe", "slack.exe", "chrome.exe", "code.exe", "windowsterminal.exe", "outlook.exe", "claude.exe"];

export default function Apps({ settings, save }: PageProps) {
  const [exe, setExe] = useState("");
  const profiles = settings.app_profiles;
  const update = (i: number, patch: Partial<AppProfile>) => save({ ...settings, app_profiles: profiles.map((p, k) => (k === i ? { ...p, ...patch } : p)) });
  const add = (name: string) => {
    const n = name.trim().toLowerCase();
    if (!n) return;
    const exeName = n.endsWith(".exe") ? n : `${n}.exe`;
    if (profiles.some((p) => p.exe === exeName)) return;
    save({ ...settings, app_profiles: [...profiles, { exe: exeName, auto_send: null, style: null }] });
    setExe("");
  };

  return (
    <>
      <div className="page-head">
        <h1>Apps</h1>
        <p>Different behaviour per app: press Enter after dictating in Discord, all lowercase in chats, and so on.</p>
      </div>
      <div className="searchbar">
        <input className="input" style={{ width: 280 }} value={exe} placeholder="App name, e.g. discord.exe" onChange={(e) => setExe(e.target.value)} onKeyDown={(e) => e.key === "Enter" && add(exe)} />
        <button className="btn" onClick={() => add(exe)}>Add app</button>
      </div>
      <div className="chips">
        {SUGGEST.filter((s) => !profiles.some((p) => p.exe === s)).map((s) => (
          <button key={s} className="chip add" onClick={() => add(s)}>+ {s.replace(".exe", "")}</button>
        ))}
      </div>
      {profiles.length > 0 && (
        <div className="section rows">
          {profiles.map((p, i) => (
            <Row key={p.exe} title={p.exe.replace(/\.exe$/, "")} desc={p.exe}>
              <Select width={160} value={p.auto_send ?? ""} options={SEND} onChange={(v) => update(i, { auto_send: (v || null) as AutoSend | null })} />
              <Select width={160} value={p.style ?? ""} options={STYLE} onChange={(v) => update(i, { style: v || null })} />
              <button className="btn ghost sm" onClick={() => save({ ...settings, app_profiles: profiles.filter((_, k) => k !== i) })}>Remove</button>
            </Row>
          ))}
        </div>
      )}
    </>
  );
}
