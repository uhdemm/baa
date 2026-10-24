// The dictation bar (design panels 01-03): pixel face, live waveform, status, and the
// expanded transcript card while you talk.
import React, { useEffect, useRef, useState } from "react";
import ReactDOM from "react-dom/client";
import { listen } from "@tauri-apps/api/event";
import Face, { Mood } from "./ui/Face";
import Wave from "./ui/Wave";
import { invoke } from "@tauri-apps/api/core";
import { applyAppearance, type Appearance } from "./appearance";
import "./styles.css";

document.documentElement.classList.add("overlay");

type State = "idle" | "listen" | "think" | "send" | "nothing" | "error" | "cancel";
interface OverlayEvent { state: State; message: string | null; kind: "dictate" | "command"; sounds: boolean }

const VIEW: Record<State, { mood: Mood; wave: "listening" | "processing" | "idle" | "calm"; label: string }> = {
  idle: { mood: "idle", wave: "idle", label: "" },
  listen: { mood: "listening", wave: "listening", label: "Listening..." },
  think: { mood: "thinking", wave: "processing", label: "Processing..." },
  send: { mood: "success", wave: "calm", label: "Inserted" },
  nothing: { mood: "idle", wave: "idle", label: "Didn't catch that" },
  error: { mood: "thinking", wave: "idle", label: "Something went wrong" },
  cancel: { mood: "idle", wave: "idle", label: "Cancelled" },
};

const SOUND: Partial<Record<State, string>> = { listen: "start", think: "stop", send: "send", error: "error", cancel: "cancel" };
const audio: Record<string, HTMLAudioElement> = {};
for (const n of new Set(Object.values(SOUND))) { const a = new Audio(`/sounds/${n}.wav`); a.preload = "auto"; audio[n] = a; }
const play = (n?: string) => { if (!n) return; const a = audio[n]; a.currentTime = 0; a.play().catch(() => {}); };

// Dev preview in a plain browser: overlay.html?state=listen&live=Some+words&kind=command
const demo = import.meta.env.DEV && !(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ ? new URLSearchParams(location.search) : null;

function Overlay() {
  const [ev, setEv] = useState<OverlayEvent>({ state: (demo?.get("state") as State) || "idle", message: demo?.get("msg") ?? null, kind: (demo?.get("kind") as "command") || "dictate", sounds: false });
  const [level, setLevel] = useState(demo ? 0.55 : 0);
  const [live, setLive] = useState(demo?.get("live") ?? "");
  const smooth = useRef(0);
  const prev = useRef<State>("idle");
  const body = useRef<HTMLDivElement>(null);

  // the pop-up wears the same material/accent/size as the app, and restyles live
  useEffect(() => {
    if (demo) return;
    invoke<{ appearance: Appearance }>("get_settings").then((s) => s.appearance && applyAppearance(s.appearance)).catch(() => {});
    const un = listen<Appearance>("appearance-changed", (e) => applyAppearance(e.payload));
    return () => void un.then((f) => f());
  }, []);

  useEffect(() => {
    if (demo) return;
    const a = listen<OverlayEvent>("overlay-state", (e) => {
      const p = e.payload;
      if (p.sounds && p.state !== prev.current) play(SOUND[p.state]); // "think" fires twice for "Polishing…"
      prev.current = p.state;
      setEv(p);
      if (p.state === "listen") setLive("");
    });
    const b = listen<number>("mic-level", (e) => { smooth.current = smooth.current * 0.55 + e.payload * 0.45; setLevel(smooth.current); });
    const c = listen<string>("live-text", (e) => setLive(e.payload));
    return () => { a.then((f) => f()); b.then((f) => f()); c.then((f) => f()); };
  }, []);
  useEffect(() => { body.current?.scrollTo({ top: 1e6 }); }, [live]);

  const { state, message, kind } = ev;
  if (state === "idle") return null;
  const v = VIEW[state];
  const label = message ?? (kind === "command" && state === "listen" ? "Say what to change..." : v.label);
  const wave = <Wave style={v.wave} level={state === "listen" ? level : undefined} />;
  const expanded = !!live && (state === "listen" || state === "think");

  const head = (
    <div className="dbar-row">
      <div className="face-wrap"><Face mood={v.mood} size={29} extras={state !== "listen"} /></div>
      {wave}
      <div className="divider" />
      <span className={`label ${state === "error" ? "err" : ""}`}>
        {kind === "command" && <b className="cmd">Command</b>}
        {expanded ? live.split(/\s+/).slice(0, 4).join(" ") + "..." : label}
      </span>
      {state === "listen" && <i className="caret" />}
    </div>
  );

  return (
    <div className="ov">
      {expanded ? (
        <div className="xcard">
          {head}
          <div className="xbody" ref={body}>{live}</div>
          <div className="xfoot">
            <span className="esc">Esc</span><span className="muted">Cancel</span>
            <span className="ins">{state === "think" ? "Inserting…" : "Let go to insert"}</span>
          </div>
        </div>
      ) : (
        <div className={`dbar ov-bar ${state}`}>{head}</div>
      )}
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Overlay />
  </React.StrictMode>,
);
