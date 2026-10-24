import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { api, DownloadProgress, EngineStatus, fmtSize, ModelRow } from "../api";
import type { PageProps } from "../App";
import { Icon } from "../ui/controls";

const TIERS: [string, string, string][] = [
  ["recommended", "Recommended", "The best fit for most people."],
  ["great", "Great with a graphics card", "More accurate, a little heavier."],
  ["light", "Light", "For older or slower PCs."],
  ["more", "More models", "Bigger or more specialised."],
  ["speakers", "Meeting speakers", "An add-on for meeting notes: works out who said what. Not a dictation model."],
  ["imported", "Your imported models", "Files you added yourself."],
];

export default function Models({ settings, save }: PageProps) {
  const [rows, setRows] = useState<ModelRow[]>([]);
  const [progress, setProgress] = useState<Record<string, DownloadProgress>>({});
  const [engine, setEngine] = useState<EngineStatus | null>(null);
  const [err, setErr] = useState<string | null>(null);

  const load = () => { api.listModels().then(setRows); api.engineStatus().then(setEngine); };
  useEffect(() => {
    load();
    const un = listen<DownloadProgress>("model-download", (e) => {
      setProgress((p) => ({ ...p, [e.payload.id]: e.payload }));
      if (e.payload.state !== "downloading" && e.payload.state !== "verifying") load();
      if (e.payload.state === "error") setErr(e.payload.error);
    });
    const t = setInterval(() => api.engineStatus().then(setEngine), 3000);
    return () => { un.then((f) => f()); clearInterval(t); };
  }, []);

  const download = (id: string) => {
    setErr(null);
    api.downloadModel(id).catch((e) => setErr(String(e))).finally(load);
    setTimeout(load, 300);
  };
  const importFile = async () => {
    const path = await open({ multiple: false, filters: [{ name: "Speech model", extensions: ["gguf", "bin"] }] });
    if (typeof path !== "string") return;
    setErr(null);
    try { await api.importModel(path); load(); } catch (e) { setErr(String(e)); }
  };

  return (
    <>
      <div className="page-head" style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start", gap: 20 }}>
        <div>
          <h1>Models</h1>
          <p>
            Speech models run on your PC, so nothing you say leaves it.{" "}
            {engine?.loaded ? <>Loaded now on <b>{engine.backend}</b>.</> : "Nothing loaded right now; a model loads the moment you talk."}
          </p>
        </div>
        <button className="btn" onClick={importFile}><Icon name="download" size={14} />Import a model file</button>
      </div>
      {err && <div className="banner err" onClick={() => setErr(null)}>{err}</div>}
      {TIERS.map(([tier, title, lead]) => {
        const list = rows.filter((r) => r.tier === tier);
        if (!list.length) return null;
        return (
          <div className="section" key={tier}>
            <h2>{title}</h2>
            <p className="lead">{lead}</p>
            <ul className="models">
              {list.map((m) => {
                const p = progress[m.id];
                const active = settings.model === m.id;
                const busy = m.downloading || p?.state === "downloading" || p?.state === "verifying";
                return (
                  <li key={m.id}>
                    <div className="grow">
                      <div><b>{m.name}</b><span className="size">{fmtSize(m.size)}</span> {(active || (m.tier === "speakers" && m.installed)) && <span className="tag">In use</span>}</div>
                      <p>{m.description}</p>
                      {busy && p && (
                        <>
                          <div className="bar-prog"><i style={{ width: `${p.total ? (p.done / p.total) * 100 : 0}%` }} /></div>
                          <div className="prog-label">{p.state === "verifying" ? "Checking the file…" : `${fmtSize(p.done)} of ${fmtSize(p.total)}`}</div>
                        </>
                      )}
                    </div>
                    {m.installed && !active && m.tier !== "speakers" && <button className="btn soft" onClick={() => save({ ...settings, model: m.id })}>Use</button>}
                    {!m.installed && !busy && <button className="btn" onClick={() => download(m.id)}>Download</button>}
                    {busy && <button className="btn ghost" onClick={() => api.cancelDownload(m.id)}>Cancel</button>}
                    {m.installed && <button className="btn ghost" onClick={() => api.deleteModel(m.id).then(load)}>{m.imported ? "Remove" : "Delete"}</button>}
                  </li>
                );
              })}
            </ul>
          </div>
        );
      })}
    </>
  );
}
