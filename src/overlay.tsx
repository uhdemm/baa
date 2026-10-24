// The bar the character sits on. Functional version for now: a mood label and
// a live mic meter. The sheep sprite player drops in here later.
import React, { useEffect, useRef, useState } from "react";
import ReactDOM from "react-dom/client";
import { listen } from "@tauri-apps/api/event";

type State = "idle" | "listen" | "think" | "send" | "nothing" | "error" | "cancel";
const LABEL: Record<State, string> = {
  idle: "",
  listen: "Listening",
  think: "Thinking…",
  send: "Baa!",
  nothing: "Didn't hear anything",
  error: "Something went wrong",
  cancel: "Cancelled",
};

function Overlay() {
  const [state, setState] = useState<State>("idle");
  const [msg, setMsg] = useState<string | null>(null);
  const [level, setLevel] = useState(0);
  const smooth = useRef(0);

  useEffect(() => {
    const a = listen<{ state: State; message: string | null }>("overlay-state", (e) => {
      setState(e.payload.state);
      setMsg(e.payload.message);
      if (e.payload.state !== "listen") setLevel(0);
    });
    const b = listen<number>("mic-level", (e) => {
      smooth.current = smooth.current * 0.5 + e.payload * 0.5;
      setLevel(smooth.current);
    });
    return () => {
      a.then((f) => f());
      b.then((f) => f());
    };
  }, []);

  if (state === "idle") return null;
  return (
    <div className="ov">
      <div className={`pill ${state}`}>
        {state === "listen" ? (
          <div className="meter">
            {Array.from({ length: 9 }, (_, i) => {
              const h = Math.max(3, level * 22 * (1 - Math.abs(i - 4) / 6));
              return <i key={i} style={{ height: h }} />;
            })}
          </div>
        ) : (
          <span>{msg ?? LABEL[state]}</span>
        )}
      </div>
    </div>
  );
}

const css = `
html,body{margin:0;background:transparent;overflow:hidden;font:600 13px/1.2 system-ui,"Segoe UI",sans-serif}
.ov{position:fixed;inset:0;display:flex;align-items:flex-end;justify-content:center;padding-bottom:6px}
.pill{min-width:120px;max-width:244px;height:36px;padding:0 16px;border-radius:18px;background:rgba(20,20,22,.92);color:#fff;
  display:flex;align-items:center;justify-content:center;box-shadow:0 4px 18px rgba(0,0,0,.25);text-align:center}
.pill.error{background:#b3261e}
.pill.send{background:#1f8a4c}
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
