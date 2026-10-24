import { useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { AiSettings, AiStatus, api } from "../api";
import type { PageProps } from "../App";

interface Provider {
  id: AiSettings["provider"];
  label: string;
  model: string; // a sensible starting model; "Load models" shows the full list
  needsKey: boolean;
  keyUrl?: string;
  url?: string;
  free?: boolean;
}

const PROVIDERS: Provider[] = [
  { id: "groq", label: "Groq", model: "llama-3.3-70b-versatile", needsKey: true, free: true, keyUrl: "https://console.groq.com/keys" },
  { id: "gemini", label: "Google Gemini", model: "gemini-2.5-flash", needsKey: true, free: true, keyUrl: "https://aistudio.google.com/apikey" },
  { id: "cerebras", label: "Cerebras", model: "gpt-oss-120b", needsKey: true, free: true, keyUrl: "https://cloud.cerebras.ai" },
  { id: "openrouter", label: "OpenRouter (free models end in :free)", model: "", needsKey: true, free: true, keyUrl: "https://openrouter.ai/keys" },
  { id: "freellmapi", label: "FreeLLMAPI (your local free-tier pool)", model: "", needsKey: false, free: true, url: "http://localhost:3001/v1" },
  { id: "ollama", label: "Ollama (runs on this PC)", model: "", needsKey: false, free: true, url: "http://localhost:11434/v1" },
  { id: "anthropic", label: "Claude (Anthropic)", model: "claude-opus-5-5", needsKey: true, keyUrl: "https://console.anthropic.com/settings/keys" },
  { id: "openai", label: "OpenAI", model: "", needsKey: true, keyUrl: "https://platform.openai.com/api-keys" },
  { id: "custom", label: "Other (OpenAI-compatible server)", model: "", needsKey: false },
];

const SAMPLE = "um so i was thinking we could uh move the meeting to thursday no wait friday and then send the notes to big square";

export default function Ai({ settings, save }: PageProps) {
  const ai = settings.ai;
  const [status, setStatus] = useState<AiStatus | null>(null);
  const [key, setKey] = useState("");
  const [sample, setSample] = useState(SAMPLE);
  const [result, setResult] = useState<string | null>(null);
  const [testing, setTesting] = useState(false);
  const [models, setModels] = useState<string[]>([]);
  const [modelErr, setModelErr] = useState<string | null>(null);
  const provider = PROVIDERS.find((p) => p.id === ai.provider) ?? PROVIDERS[0];

  const refresh = () => api.aiStatus().then(setStatus);
  useEffect(() => void refresh(), [settings]);

  const set = (patch: Partial<AiSettings>) => save({ ...settings, ai: { ...ai, ...patch } });
  const hasKey = status?.keys.includes(ai.provider);

  const saveKey = async () => {
    await api.setApiKey(ai.provider, key);
    setKey("");
    refresh();
  };

  const test = async () => {
    setTesting(true);
    setResult(null);
    try {
      setResult(await api.testAi(sample));
    } catch (e) {
      setResult(`Error: ${e}`);
    }
    setTesting(false);
  };

  return (
    <>
      <h1>AI</h1>
      <p className="muted">
        Optional. Speech-to-text always runs on your PC; this adds an AI pass on top. It powers <b>clean-up</b> (fixes
        grammar, removes false starts, formats lists) and <b>command mode</b>: select text anywhere, hold{" "}
        <kbd>{settings.command_shortcut}</kbd> and say what to do ("make this shorter", "translate to Spanish"). With
        nothing selected, it writes new text.
      </p>
      {status && (
        <div className={`banner ${status.ready ? "" : "warn"}`}>
          {status.ready ? "AI is set up." : "Not set up yet: add a key below, or pick Ollama to run a model on this PC."}
        </div>
      )}
      <div className="form">
        <label>Provider</label>
        <select
          value={ai.provider}
          onChange={(e) => {
            const p = PROVIDERS.find((x) => x.id === e.target.value)!;
            set({ provider: p.id, model: p.model, base_url: "" });
            setModels([]);
            setModelErr(null);
          }}
        >
          {PROVIDERS.map((p) => <option key={p.id} value={p.id}>{p.label}{p.free ? " (free)" : ""}</option>)}
        </select>

        {(provider.needsKey || ai.provider === "custom" || ai.provider === "freellmapi") && (
          <>
            <label>API key</label>
            <div className="row">
              <input
                type="password"
                value={key}
                placeholder={hasKey ? "Saved (type to replace)" : "Paste your key"}
                onChange={(e) => setKey(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && key && saveKey()}
              />
              <button disabled={!key} onClick={saveKey}>Save</button>
              {hasKey && <button className="ghost" onClick={() => api.setApiKey(ai.provider, "").then(refresh)}>Remove</button>}
              {provider.keyUrl && <button className="ghost" onClick={() => openUrl(provider.keyUrl!)}>Get a key</button>}
            </div>
          </>
        )}

        {(ai.provider === "ollama" || ai.provider === "custom" || ai.provider === "freellmapi") && (
          <>
            <label>Server address</label>
            <input value={ai.base_url} placeholder={provider.url ?? "https://…/v1"} onChange={(e) => set({ base_url: e.target.value })} />
          </>
        )}

        <label>Model</label>
        <div className="row">
          <input list="ai-models" value={ai.model} placeholder="model name" onChange={(e) => set({ model: e.target.value })} />
          <datalist id="ai-models">{models.map((m) => <option key={m} value={m} />)}</datalist>
          <button
            className="ghost"
            onClick={() => {
              setModelErr(null);
              api.listAiModels().then((m) => { setModels(m); if (!m.length) setModelErr("No models listed"); }).catch((e) => setModelErr(String(e)));
            }}
          >
            Load models
          </button>
          {models.length > 0 && <span className="muted sm">{models.length} available, pick from the box</span>}
          {modelErr && <span className="sm" style={{ color: "var(--err)" }}>{modelErr}</span>}
        </div>

        <label>Clean up dictations</label>
        <label className="check">
          <input type="checkbox" checked={ai.cleanup} onChange={(e) => set({ cleanup: e.target.checked })} />
          Run every dictation through the AI before pasting (adds a moment; if the AI is slow or offline, Baa pastes what you said instead)
        </label>

        <label>Give up after</label>
        <select value={ai.timeout_secs} onChange={(e) => set({ timeout_secs: Number(e.target.value) })}>
          {[3, 6, 10, 20].map((n) => <option key={n} value={n}>{n} seconds</option>)}
        </select>
      </div>

      <section>
        <h2>Clean-up instructions</h2>
        <textarea className="prompt" value={ai.prompt} onChange={(e) => set({ prompt: e.target.value })} />
      </section>

      <section>
        <h2>Try it</h2>
        <textarea value={sample} onChange={(e) => setSample(e.target.value)} />
        <div className="row" style={{ marginTop: 8 }}>
          <button onClick={test} disabled={testing || !status?.ready}>{testing ? "Asking…" : "Clean this up"}</button>
        </div>
        {result !== null && <p className="preview">{result}</p>}
      </section>
    </>
  );
}
