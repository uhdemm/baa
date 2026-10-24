import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, DownloadProgress, fmtSize, ModelRow } from "../api";
import type { PageProps } from "../App";
import Face from "../ui/Face";
import { Icon, Keys, Select } from "../ui/controls";

// First run: microphone -> speech model -> try it.
const PICKS = ["parakeet-tdt-0.6b-v3", "parakeet-unified-en-0.6b", "parakeet-tdt_ctc-110m"];

export default function Setup({ settings, save }: PageProps) {
  const [step, setStep] = useState(0);
  const finish = () => save({ ...settings, onboarded: true });
  return (
    <div className="setup">
      <div className="setup-card">
        {step === 0 && <MicStep settings={settings} save={save} />}
        {step === 1 && <ModelStep settings={settings} save={save} />}
        {step === 2 && <TryStep settings={settings} save={save} />}
        <div className="pager">{[0, 1, 2].map((i) => <i key={i} className={i === step ? "on" : ""} />)}</div>
        <div className="setup-foot">
          <button className="btn ghost" onClick={finish}>Skip</button>
          {step < 2
            ? <button className="btn primary" disabled={step === 1 && !modelReady(settings)} onClick={() => setStep(step + 1)}>Continue</button>
            : <button className="btn primary" onClick={finish}>Done</button>}
        </div>
      </div>
    </div>
  );
}

let installed: string[] = [];
const modelReady = (s: PageProps["settings"]) => !!s.model && installed.includes(s.model);

function MicStep({ settings, save }: PageProps) {
  const [mics, setMics] = useState<string[]>([]);
  const [level, setLevel] = useState(0);
  const [err, setErr] = useState<string | null>(null);
  const peak = useRef(0);
  useEffect(() => {
    api.listMicrophones().then(setMics);
    const un = listen<number>("mic-level", (e) => { peak.current = Math.max(peak.current * 0.9, e.payload); setLevel(peak.current); });
    return () => { un.then((f) => f()); api.micTestStop(); };
  }, []);
  useEffect(() => {
    const t = setTimeout(() => api.micTestStop().then(() => api.micTestStart()).then(() => setErr(null)).catch((e) => setErr(String(e))), 300);
    return () => clearTimeout(t);
  }, [settings.microphone]);
  return (
    <>
      <Face size={38} mood={level > 0.15 ? "listening" : "idle"} />
      <h1>Let's get you set up.</h1>
      <p>Baa turns what you say into text, right where you're typing. Everything runs on this PC.</p>
      <div className="perm">
        <div className="pi"><Icon name="mic" size={20} /></div>
        <div className="grow">
          <div><b>Microphone</b><span>Say something. The bar should move.</span></div>
          <Select width={320} value={settings.microphone ?? ""} onChange={(v) => save({ ...settings, microphone: v || null })}
            options={[{ value: "", label: "Windows default" }, ...mics.map((m) => ({ value: m, label: m }))]} />
          <div className="level"><i style={{ width: `${Math.min(100, level * 100)}%` }} /></div>
          {err && <span style={{ color: "var(--danger)" }}>{err}</span>}
        </div>
      </div>
    </>
  );
}

function ModelStep({ settings, save }: PageProps) {
  const [rows, setRows] = useState<ModelRow[]>([]);
  const [prog, setProg] = useState<Record<string, DownloadProgress>>({});
  const [err, setErr] = useState<string | null>(null);
  const load = () => api.listModels().then((r) => { installed = r.filter((m) => m.installed).map((m) => m.id); setRows(r); });
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
  return (
    <>
      <Face size={38} mood="thinking" />
      <h1>Pick a speech model.</h1>
      <p>It downloads once and runs on your PC. You can add others later.</p>
      <ul className="pick" style={{ marginTop: 22 }}>
        {picks.map((m) => {
          const p = prog[m.id];
          const busy = p && (p.state === "downloading" || p.state === "verifying");
          const active = settings.model === m.id;
          return (
            <li key={m.id} className={active && m.installed ? "active" : ""}>
              <div className="grow">
                <b>{m.name}</b> <span className="size">{fmtSize(m.size)}</span>
                <p>{m.description}</p>
                {busy && <><div className="bar-prog"><i style={{ width: `${p.total ? (p.done / p.total) * 100 : 0}%` }} /></div>
                  <div className="prog-label">{p.state === "verifying" ? "Checking the file…" : `${fmtSize(p.done)} of ${fmtSize(p.total)}`}</div></>}
              </div>
              {m.installed
                ? active ? <span className="tag">Selected</span> : <button className="btn soft sm" onClick={() => save({ ...settings, model: m.id })}>Use</button>
                : !busy && <button className="btn sm" onClick={() => { setErr(null); api.downloadModel(m.id).then(() => { load(); save({ ...settings, model: m.id }); }).catch((e) => setErr(String(e))); }}>Download</button>}
            </li>
          );
        })}
      </ul>
      {err && <p style={{ color: "var(--danger)", marginTop: 10 }}>{err}</p>}
    </>
  );
}

function TryStep({ settings }: PageProps) {
  const [said, setSaid] = useState(false);
  const box = useRef<HTMLTextAreaElement>(null);
  useEffect(() => {
    box.current?.focus();
    const un = listen("history-changed", () => setSaid(true));
    return () => void un.then((f) => f());
  }, []);
  return (
    <>
      <Face size={38} mood={said ? "success" : "happy"} />
      <h1>{said ? "That's it. You're set." : "Try it."}</h1>
      <p>Click in the box, hold <Keys combo={settings.shortcut} /> and say “Hello Baa, this is my first dictation”.</p>
      <textarea ref={box} className="trybox" placeholder="Your words land here" />
    </>
  );
}
