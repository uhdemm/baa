import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, DownloadProgress, fmtSize, ModelRow } from "../api";
import type { PageProps } from "../App";

// The first-run flow: mic -> model -> try it.
const PICKS = ["parakeet-tdt-0.6b-v3", "parakeet-unified-en-0.6b", "parakeet-tdt_ctc-110m"];

export default function Setup({ settings, save }: PageProps) {
  const [step, setStep] = useState(0);
  return (
    <div className="setup">
      <div className="steps">
        {["Microphone", "Speech model", "Try it"].map((s, i) => (
          <span key={s} className={i === step ? "on" : i < step ? "done" : ""}>{i + 1}. {s}</span>
        ))}
      </div>
      {step === 0 && <MicStep {...{ settings, save }} next={() => setStep(1)} />}
      {step === 1 && <ModelStep {...{ settings, save }} next={() => setStep(2)} />}
      {step === 2 && <TryStep {...{ settings, save }} finish={() => save({ ...settings, onboarded: true })} />}
    </div>
  );
}

function MicStep({ settings, save, next }: PageProps & { next: () => void }) {
  const [mics, setMics] = useState<string[]>([]);
  const [level, setLevel] = useState(0);
  const [err, setErr] = useState<string | null>(null);
  const peak = useRef(0);

  useEffect(() => {
    api.listMicrophones().then(setMics);
    const un = listen<number>("mic-level", (e) => {
      peak.current = Math.max(peak.current * 0.9, e.payload);
      setLevel(peak.current);
    });
    return () => {
      un.then((f) => f());
      api.micTestStop();
    };
  }, []);

  // restart the test whenever the chosen mic changes
  useEffect(() => {
    api.micTestStop().then(() => api.micTestStart()).then(() => setErr(null)).catch((e) => setErr(String(e)));
  }, [settings.microphone]);

  return (
    <section className="card">
      <h1>Hi, I'm Baa.</h1>
      <p className="muted">Hold a shortcut, talk, and your words appear wherever you're typing. Everything runs on this PC.</p>
      <h2>Which microphone?</h2>
      <select value={settings.microphone ?? ""} onChange={(e) => save({ ...settings, microphone: e.target.value || null })}>
        <option value="">Windows default</option>
        {mics.map((m) => <option key={m} value={m}>{m}</option>)}
      </select>
      <p className="muted">Say something. The bar should move.</p>
      <div className="level"><i style={{ width: `${Math.min(100, level * 100)}%` }} /></div>
      {err && <div className="banner err">{err}</div>}
      <div className="row end"><button onClick={next}>Sounds good</button></div>
    </section>
  );
}

function ModelStep({ settings, save, next }: PageProps & { next: () => void }) {
  const [rows, setRows] = useState<ModelRow[]>([]);
  const [prog, setProg] = useState<Record<string, DownloadProgress>>({});
  const [err, setErr] = useState<string | null>(null);
  const load = () => api.listModels().then(setRows);

  useEffect(() => {
    load();
    const un = listen<DownloadProgress>("model-download", (e) => {
      setProg((p) => ({ ...p, [e.payload.id]: e.payload }));
      if (e.payload.state === "error") setErr(e.payload.error);
      if (e.payload.state === "done") load();
    });
    return () => void un.then((f) => f());
  }, []);

  const picks = PICKS.map((id) => rows.find((r) => r.id === id)).filter(Boolean) as ModelRow[];
  const ready = !!settings.model && rows.some((r) => r.id === settings.model && r.installed);

  return (
    <section className="card">
      <h1>Pick a speech model</h1>
      <p className="muted">It's downloaded once and runs on your PC. You can add others later under Models.</p>
      <ul className="models">
        {picks.map((m) => {
          const p = prog[m.id];
          const busy = p && (p.state === "downloading" || p.state === "verifying");
          const active = settings.model === m.id;
          return (
            <li key={m.id} className={active ? "active" : ""}>
              <div className="grow">
                <b>{m.name}</b> <span className="muted sm">{fmtSize(m.size)}</span>
                <p className="muted sm">{m.description}</p>
                {busy && (
                  <div className="bar">
                    <i style={{ width: `${p.total ? (p.done / p.total) * 100 : 0}%` }} />
                    <span>{p.state === "verifying" ? "Checking file…" : `${fmtSize(p.done)} of ${fmtSize(p.total)}`}</span>
                  </div>
                )}
              </div>
              {m.installed ? (
                active ? <span className="tag">Selected</span> : <button onClick={() => save({ ...settings, model: m.id })}>Use</button>
              ) : (
                !busy && (
                  <button onClick={() => { setErr(null); api.downloadModel(m.id).then(() => save({ ...settings, model: m.id })).catch((e) => setErr(String(e))); }}>
                    Download
                  </button>
                )
              )}
            </li>
          );
        })}
      </ul>
      {err && <div className="banner err">{err}</div>}
      <div className="row end"><button disabled={!ready} onClick={next}>Next</button></div>
    </section>
  );
}

function TryStep({ settings, finish }: PageProps & { finish: () => void }) {
  const [said, setSaid] = useState<string | null>(null);
  const box = useRef<HTMLTextAreaElement>(null);
  useEffect(() => {
    box.current?.focus();
    const un = listen("history-changed", () => api.historyList(1, 0).then((r) => setSaid(r[0]?.text ?? "")));
    return () => void un.then((f) => f());
  }, []);
  return (
    <section className="card">
      <h1>Try it</h1>
      <p>
        Click in the box, then <b>hold <kbd>{settings.shortcut}</kbd></b>, say "Hello Baa, this is my first dictation", and let go.
        Or tap it once to start and again to stop.
      </p>
      <textarea ref={box} className="trybox" placeholder="Your words land here" />
      {said !== null && <div className="banner ok">It works. You can change the shortcut any time in Settings.</div>}
      <div className="row end">
        <button className="ghost" onClick={finish}>Skip</button>
        <button disabled={said === null} onClick={finish}>Done</button>
      </div>
    </section>
  );
}
