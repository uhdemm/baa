import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { api, DownloadProgress, EngineStatus, fmtSize, ModelRow } from "../api";
import type { PageProps } from "../App";

const TIERS: [string, string][] = [
  ["recommended", "Recommended"],
  ["great", "Great with a GPU"],
  ["light", "Light: older or slower PCs"],
  ["more", "More models"],
  ["imported", "Your imported models"],
];

export default function Models({ settings, save }: PageProps) {
  const [rows, setRows] = useState<ModelRow[]>([]);
  const [progress, setProgress] = useState<Record<string, DownloadProgress>>({});
  const [engine, setEngine] = useState<EngineStatus | null>(null);
  const [err, setErr] = useState<string | null>(null);

  const load = () => {
    api.listModels().then(setRows);
    api.engineStatus().then(setEngine);
  };

  useEffect(() => {
    load();
    const un = listen<DownloadProgress>("model-download", (e) => {
      setProgress((p) => ({ ...p, [e.payload.id]: e.payload }));
      if (e.payload.state !== "downloading" && e.payload.state !== "verifying") load();
      if (e.payload.state === "error") setErr(e.payload.error);
    });
    const t = setInterval(() => api.engineStatus().then(setEngine), 3000);
    return () => {
      un.then((f) => f());
      clearInterval(t);
    };
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
    try {
      await api.importModel(path);
      load();
    } catch (e) {
      setErr(String(e));
    }
  };

  return (
    <>
      <div className="row between">
        <h1>Models</h1>
        <button onClick={importFile}>Import a model file…</button>
      </div>
      <p className="muted">
        Models run on your own computer. Nothing you say leaves it.
        {engine?.loaded && <> Loaded now on <b>{engine.backend}</b>.</>}
        {engine && !engine.loaded && <> No model in memory right now (it loads when you talk).</>}
      </p>
      {err && <div className="banner err">{err}</div>}
      {TIERS.map(([tier, title]) => {
        const list = rows.filter((r) => r.tier === tier);
        if (!list.length) return null;
        return (
          <section key={tier}>
            <h2>{title}</h2>
            <ul className="models">
              {list.map((m) => {
                const p = progress[m.id];
                const active = settings.model === m.id;
                const busy = m.downloading || p?.state === "downloading" || p?.state === "verifying";
                return (
                  <li key={m.id} className={active ? "active" : ""}>
                    <div className="grow">
                      <div className="row">
                        <b>{m.name}</b>
                        {active && <span className="tag">In use</span>}
                        <span className="muted sm">{fmtSize(m.size)}</span>
                      </div>
                      <p className="muted sm">{m.description}</p>
                      {busy && p && (
                        <div className="bar">
                          <i style={{ width: `${p.total ? (p.done / p.total) * 100 : 0}%` }} />
                          <span>{p.state === "verifying" ? "Checking file…" : `${fmtSize(p.done)} of ${fmtSize(p.total)}`}</span>
                        </div>
                      )}
                    </div>
                    <div className="row">
                      {m.installed && !active && (
                        <button onClick={() => save({ ...settings, model: m.id })}>Use</button>
                      )}
                      {!m.installed && !busy && <button onClick={() => download(m.id)}>Download</button>}
                      {busy && <button className="ghost" onClick={() => api.cancelDownload(m.id)}>Cancel</button>}
                      {m.installed && (
                        <button className="ghost" onClick={() => api.deleteModel(m.id).then(load)}>
                          {m.imported ? "Remove" : "Delete"}
                        </button>
                      )}
                    </div>
                  </li>
                );
              })}
            </ul>
          </section>
        );
      })}
    </>
  );
}
