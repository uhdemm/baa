// The bar the character sits on. Functional version for now: mood label, live mic
// meter, the words so far, and sounds. The sheep sprite player drops in here later.
import React, { useEffect, useRef, useState } from "react";
import ReactDOM from "react-dom/client";
import { listen } from "@tauri-apps/api/event";

type State = "idle" | "listen" | "think" | "send" | "nothing" | "error" | "cancel";
interface OverlayEvent { state: State; message: string | null; kind: "dictate" | "command"; sounds: boolean }

const LABEL: Record<State, string> = {
  idle: "",
  listen: "Listening",
  think: "Thinking…",
  send: "Baa!",
  nothing: "Didn't hear anything",
  error: "Something went wrong",
  cancel: "Cancelled",
};

// Which sound plays when the bar enters a state.
const SOUND: Partial<Record<State, string>> = {
  listen: "start", think: "stop", send: "send", error: "error", cancel: "cancel",
};
const audio: Record<string, HTMLAudioElement> = {};
for (const n of new Set(Object.values(SOUND))) {
  const a = new Audio(`/sounds/${n}.wav`);
  a.preload = "auto";
  audio[n] = a;
}
function play(name: string | undefined) {
  if (!name) return;
  const a = audio[name];
  a.currentTime = 0;
  a.play().catch(() => {});
}

function Overlay() {
  const [ev, setEv] = useState<OverlayEvent>({ state: "idle", message: null, kind: "dictate", sounds: true });
  const [level, setLevel] = useState(0);
  const [live, setLive] = useState("");
  const smooth = useRef(0);
  const prev = useRef<State>("idle");

  useEffect(() => {
    const a = listen<OverlayEvent>("overlay-state", (e) => {
      const p = e.payload;
      // "think" fires again for "Polishing…"; only sound the first time
      if (p.sounds && p.state !== prev.current) play(SOUND[p.state]);
      prev.current = p.state;
      setEv(p);
      if (p.state === "listen") setLive("");
      if (p.state !== "listen") setLevel(0);
    });
    const b = listen<number>("mic-level", (e) => {
      smooth.current = smooth.current * 0.5 + e.payload * 0.5;
      setLevel(smooth.current);
    });
    const c = listen<string>("live-text", (e) => setLive(e.payload));
    return () => {
      a.then((f) => f());
      b.then((f) => f());
      c.then((f) => f());
    };
  }, []);

  const { state, message, kind } = ev;
  if (state === "idle") return null;
  const showLive = live && (state === "listen" || state === "think");
  return (
    <div className="ov">
      {showLive && <div className="live">{live.length > 140 ? "…" + live.slice(-140) : live}</div>}
      <div className={`pill ${state} ${kind}`}>
        {kind === "command" && state === "listen" && <span className="tag">Command</span>}
        {state === "listen" ? (
          <div className="meter">
            {Array.from({ length: 9 }, (_, i) => {
              const h = Math.max(3, level * 22 * (1 - Math.abs(i - 4) / 6));
              return <i key={i} style={{ height: h }} />;
            })}
          </div>
        ) : (
          <span>{message ?? LABEL[state]}</span>
        )}
      </div>
    </div>
  );
}

const css = `
html,body{margin:0;background:transparent;overflow:hidden;font:600 13px/1.25 system-ui,"Segoe UI",sans-serif}
.ov{position:fixed;inset:0;display:flex;flex-direction:column;align-items:center;justify-content:flex-end;gap:6px;padding:0 8px 6px}
.live{max-width:244px;background:rgba(20,20,22,.88);color:#fff;font-weight:500;border-radius:10px;padding:8px 11px;
  box-shadow:0 4px 18px rgba(0,0,0,.25);display:-webkit-box;-webkit-line-clamp:4;-webkit-box-orient:vertical;overflow:hidden}
.pill{min-width:120px;max-width:244px;min-height:36px;padding:6px 16px;border-radius:18px;background:rgba(20,20,22,.92);color:#fff;
  display:flex;gap:8px;align-items:center;justify-content:center;box-shadow:0 4px 18px rgba(0,0,0,.25);text-align:center;box-sizing:border-box}
.pill.error{background:#b3261e}
.pill.send{background:#1f8a4c}
.pill.command.listen{background:#3a2d8f}
.tag{font-size:11px;opacity:.8;text-transform:uppercase;letter-spacing:.06em}
.meter{display:flex;gap:4px;align-items:center;height:24px}
.meter i{display:block;width:4px;border-radius:2px;background:#fff;transition:height 60ms linear}
`;
const style = document.createElement("style");
style.textContent = css;
document.head.appendChild(style);

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Overlay />
  </React.StrictMode>,
);
