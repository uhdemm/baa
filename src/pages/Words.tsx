import { useEffect, useState } from "react";
import { api } from "../api";
import type { PageProps } from "../App";

export default function Words({ settings, save }: PageProps) {
  const [word, setWord] = useState("");
  const [test, setTest] = useState("um so send it to my email please");
  const [preview, setPreview] = useState("");

  useEffect(() => {
    api.previewText(test).then(setPreview);
  }, [test, settings]);

  const addWord = () => {
    const w = word.trim();
    if (!w || settings.custom_words.includes(w)) return;
    save({ ...settings, custom_words: [...settings.custom_words, w] });
    setWord("");
  };

  return (
    <>
      <h1>Words</h1>

      <section>
        <h2>Custom words</h2>
        <p className="muted">Names and terms the model should spell your way (works best with Parakeet and Canary).</p>
        <div className="row">
          <input value={word} placeholder="e.g. Remotion" onChange={(e) => setWord(e.target.value)} onKeyDown={(e) => e.key === "Enter" && addWord()} />
          <button onClick={addWord}>Add</button>
        </div>
        <div className="chips">
          {settings.custom_words.map((w) => (
            <span key={w} className="chip">
              {w}
              <button onClick={() => save({ ...settings, custom_words: settings.custom_words.filter((x) => x !== w) })}>×</button>
            </span>
          ))}
        </div>
      </section>

      <section>
        <h2>Replacements</h2>
        <p className="muted">When the model writes the left side, Baa types the right side.</p>
        <PairTable
          rows={settings.replacements.map((r) => [r.from, r.to])}
          labels={["It hears", "Baa types"]}
          onChange={(rows) => save({ ...settings, replacements: rows.map(([from, to]) => ({ from, to })) })}
        />
      </section>

      <section>
        <h2>Snippets</h2>
        <p className="muted">Say the trigger and Baa types the full text, like "my email" or "my address".</p>
        <PairTable
          rows={settings.snippets.map((s) => [s.trigger, s.text])}
          labels={["You say", "Baa types"]}
          multiline
          onChange={(rows) => save({ ...settings, snippets: rows.map(([trigger, text]) => ({ trigger, text })) })}
        />
      </section>

      <section>
        <h2>Try it</h2>
        <input className="wide" value={test} onChange={(e) => setTest(e.target.value)} />
        <p className="preview">{preview || " "}</p>
      </section>
    </>
  );
}

function PairTable({ rows, labels, onChange, multiline }: {
  rows: [string, string][];
  labels: [string, string];
  onChange: (rows: [string, string][]) => void;
  multiline?: boolean;
}) {
  const [draft, setDraft] = useState<[string, string]>(["", ""]);
  const set = (i: number, j: 0 | 1, v: string) => {
    const next = rows.map((r) => [...r] as [string, string]);
    next[i][j] = v;
    onChange(next);
  };
  const add = () => {
    if (!draft[0].trim()) return;
    onChange([...rows, draft]);
    setDraft(["", ""]);
  };
  const Field = multiline ? "textarea" : "input";
  return (
    <table className="pairs">
      <thead>
        <tr><th>{labels[0]}</th><th>{labels[1]}</th><th /></tr>
      </thead>
      <tbody>
        {rows.map((r, i) => (
          <tr key={i}>
            <td><input value={r[0]} onChange={(e) => set(i, 0, e.target.value)} /></td>
            <td><Field value={r[1]} onChange={(e) => set(i, 1, e.target.value)} /></td>
            <td><button className="ghost sm" onClick={() => onChange(rows.filter((_, k) => k !== i))}>Remove</button></td>
          </tr>
        ))}
        <tr>
          <td><input value={draft[0]} placeholder="…" onChange={(e) => setDraft([e.target.value, draft[1]])} /></td>
          <td><Field value={draft[1]} placeholder="…" onChange={(e) => setDraft([draft[0], e.target.value])} /></td>
          <td><button className="sm" onClick={add}>Add</button></td>
        </tr>
      </tbody>
    </table>
  );
}
