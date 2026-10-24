import { useCallback, useEffect, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { AiSettings, AiStatus, api } from "../api";
import type { PageProps } from "../App";
import { Combobox, Keys, Row, Select, Toggle } from "../ui/controls";

interface Provider {
  id: AiSettings["provider"];
  label: string;
  model: string; // a sensible starting model; the picker lists the rest
  needsKey: boolean;
  keyUrl?: string;
  url?: string;
  free?: boolean;
}

const PROVIDERS: Provider[] = [
  { id: "groq", label: "Groq", model: "llama-3.3-70b-versatile", needsKey: true, free: true, keyUrl: "https://console.groq.com/keys" },
  { id: "gemini", label: "Google Gemini", model: "gemini-2.5-flash", needsKey: true, free: true, keyUrl: "https://aistudio.google.com/apikey" },
  { id: "cerebras", label: "Cerebras", model: "gpt-oss-120b", needsKey: true, free: true, keyUrl: "https://cloud.cerebras.ai" },
  { id: "openrouter", label: "OpenRouter", model: "", needsKey: true, free: true, keyUrl: "https://openrouter.ai/keys" },
  { id: "freellmapi", label: "FreeLLMAPI (local)", model: "", needsKey: false, free: true, url: "http://localhost:3001/v1" },
  { id: "ollama", label: "Ollama (this PC)", model: "", needsKey: false, free: true, url: "http://localhost:11434/v1" },
  { id: "anthropic", label: "Claude", model: "claude-opus-5-5", needsKey: true, keyUrl: "https://console.anthropic.com/settings/keys" },
  { id: "openai", label: "OpenAI", model: "", needsKey: true, keyUrl: "https://platform.openai.com/api-keys" },
  { id: "custom", label: "Other (OpenAI-compatible)", model: "", needsKey: false },
];

const SAMPLE = "hey sarah um thanks for sending these over i looked through them and they look good just two things first can you double check the march total and second can we add the ad spend column thanks adam";

// "Pretend I'm typing into..." for the test box
const DESTINATIONS: [string, string][] = [
  ["outlook.exe: RE: Q3 numbers", "Outlook (email)"],
  ["discord.exe: #general", "Discord (chat)"],
  ["notion.exe: Meeting notes", "Notion (notes)"],
  ["Code.exe: main.rs", "VS Code"],
  ["chrome.exe: ChatGPT", "ChatGPT"],
  ["chrome.exe: Google Search", "A search box"],
];

export default function Ai({ settings, save }: PageProps) {
  const ai = settings.ai;
  const [status, setStatus] = useState<AiStatus | null>(null);
  const [key, setKey] = useState("");
  const [sample, setSample] = useState(SAMPLE);
  const [result, setResult] = useState<string | null>(null);
  const [testing, setTesting] = useState(false);
  const [dest, setDest] = useState(DESTINATIONS[0][0]);
  const provider = PROVIDERS.find((p) => p.id === ai.provider) ?? PROVIDERS[0];

  const refresh = () => api.aiStatus().then(setStatus);
  useEffect(() => { refresh(); }, [ai.provider, ai.base_url]);

  const set = (patch: Partial<AiSettings>) => save({ ...settings, ai: { ...ai, ...patch } });
  const hasKey = !!status?.keys.includes(ai.provider);
  const needsKeyField = provider.needsKey || ai.provider === "custom" || ai.provider === "freellmapi";
  const needsUrl = ai.provider === "ollama" || ai.provider === "custom" || ai.provider === "freellmapi";

  // Stable per provider/server so the picker keeps its loaded list; waits for the settings write first.
  const loadModels = useCallback(async () => {
    await new Promise((r) => setTimeout(r, 300));
    return api.listAiModels();
  }, [ai.provider, ai.base_url, hasKey]);

  const saveKey = async () => {
    await api.setApiKey(ai.provider, key);
    setKey("");
    refresh();
  };
  const test = async () => {
    setTesting(true);
    setResult(null);
    try { setResult(await api.testAi(sample, dest)); } catch (e) { setResult(`Error: ${e}`); }
    setTesting(false);
  };

  return (
    <>
      <div className="page-head">
        <h1>AI</h1>
        <p>
          Optional. Speech-to-text always runs on your PC; this adds an AI pass on top for clean-up and command mode
          (select text, hold <Keys combo={settings.command_shortcut} />, say what to do).
        </p>
      </div>
      {status && (
        <div className={`banner ${status.ready ? "ok" : "warn"}`}>
          {status.ready ? "AI is ready." : "Not set up yet. Groq, Gemini and Cerebras have free keys; Ollama runs on this PC."}
        </div>
      )}

      <div className="rows">
        <Row title="Provider" desc={provider.free ? "Free to use" : "Paid, uses your account"}>
          <Select width={240} value={ai.provider}
            options={PROVIDERS.map((p) => ({ value: p.id, label: p.label, hint: p.free ? "free" : undefined }))}
            onChange={(id) => { const p = PROVIDERS.find((x) => x.id === id)!; set({ provider: p.id, model: p.model, base_url: "" }); }} />
        </Row>

        {needsKeyField && (
          <Row title="API key" desc={hasKey ? "Saved in Windows Credential Manager" : provider.needsKey ? "Needed to use this provider" : "Only if your server asks for one"}>
            <input type="password" style={{ width: 210 }} value={key} placeholder={hasKey ? "•••••••• saved" : "Paste your key"}
              onChange={(e) => setKey(e.target.value)} onKeyDown={(e) => e.key === "Enter" && key && saveKey()} />
            {key ? <button className="btn primary" onClick={saveKey}>Save</button>
              : hasKey ? <button className="btn ghost" onClick={() => api.setApiKey(ai.provider, "").then(refresh)}>Remove</button>
              : provider.keyUrl ? <button className="btn" onClick={() => openUrl(provider.keyUrl!)}>Get a key</button> : null}
          </Row>
        )}

        {needsUrl && (
          <Row title="Server address" desc="Where the server is running">
            <input style={{ width: 300 }} value={ai.base_url} placeholder={provider.url ?? "https://…/v1"} onChange={(e) => set({ base_url: e.target.value })} />
          </Row>
        )}

        <Row title="Model" desc="Pick from the provider's list, or type a name">
          <Combobox key={ai.provider} value={ai.model} onChange={(m) => set({ model: m })} load={loadModels} placeholder="Choose a model" width={300} />
        </Row>

        <Row title="Clean up dictations" desc="Fix grammar and false starts before pasting. If the AI is slow or offline, Baa pastes what you said.">
          <Toggle on={ai.cleanup} onChange={(v) => set({ cleanup: v })} label="Clean up dictations" />
        </Row>

        <Row title="Give up after" desc="How long to wait for the AI before pasting your words as they are">
          <Select width={140} value={ai.timeout_secs} options={[3, 6, 10, 20].map((n) => ({ value: n, label: `${n} seconds` }))} onChange={(v) => set({ timeout_secs: v })} />
        </Row>
      </div>

      <div className="section">
        <h2>Clean-up instructions</h2>
        <p className="lead">What "clean up" means. Baa also tells the AI which app and window you're typing into, plus your custom words.</p>
        <textarea style={{ minHeight: 220, fontSize: 12, lineHeight: "17px" }} value={ai.prompt} onChange={(e) => set({ prompt: e.target.value })} />
        <div style={{ display: "flex", justifyContent: "flex-end" }}>
          <button className="btn" onClick={() => api.defaultCleanupPrompt().then((prompt) => set({ prompt }))}>Reset to default</button>
        </div>
      </div>

      <div className="section">
        <h2>Try it</h2>
        <p className="lead">Type something the way you'd say it, pick where it's going, and see what Baa would paste.</p>
        <textarea value={sample} onChange={(e) => setSample(e.target.value)} />
        <div style={{ marginTop: 10, display: "flex", gap: 8, alignItems: "center" }}>
          <span className="muted">Pretend I'm typing into</span>
          <Select width={190} value={dest} options={DESTINATIONS.map(([value, label]) => ({ value, label }))} onChange={setDest} />
          <button className="btn primary" onClick={test} disabled={testing || !status?.ready}>{testing ? "Asking…" : "Clean this up"}</button>
        </div>
        {result !== null && <p className="preview">{result}</p>}
      </div>
    </>
  );
}
