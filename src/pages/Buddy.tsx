import { useEffect, useState } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type { PageProps } from "../App";
import type { Settings } from "../api";
import { allCharacters, Character, CharacterInfo, loadCharacter } from "../character";
import Face from "../ui/Face";
import Sprite from "../ui/Sprite";
import { Row, Select, Toggle } from "../ui/controls";

interface Project { id: string; name: string; description: string; refs: string[]; master_ok: boolean }
interface Progress { id: string; step: "master" | "sheets" | "pack" | "done" | "error"; done: number; total: number; message: string }

/** A character card: plays its idle loop, and talks when you hover it. */
function CharCard({ info, on, onPick, onRemove }: { info: CharacterInfo; on: boolean; onPick: () => void; onRemove?: () => void }) {
  const [c, setC] = useState<Character | null>(null);
  const [hover, setHover] = useState(false);
  const [level, setLevel] = useState(0);
  useEffect(() => { loadCharacter(info.id, info.dir).then(setC).catch(() => {}); }, [info.id, info.dir]);
  useEffect(() => {
    if (!hover) return;
    const t = window.setInterval(() => setLevel(Math.random()), 110); // pretend talking
    return () => window.clearInterval(t);
  }, [hover]);
  return (
    <button className={`char-card ${on ? "on" : ""}`} onClick={onPick} onMouseEnter={() => setHover(true)} onMouseLeave={() => setHover(false)}>
      <div className="char-stage">{c ? <Sprite character={c} mood={hover ? "listen" : "idle"} level={level} height={86} /> : <Face size={30} mood="thinking" />}</div>
      <b>{info.name}</b>
      <span>{info.dir ? (info.author ? `by ${info.author}` : "Yours") : "Built in"}</span>
      {onRemove && !on && <i className="char-remove" role="button" title="Remove" onClick={(e) => { e.stopPropagation(); onRemove(); }}>×</i>}
    </button>
  );
}

/** Make your own: describe it, approve the drawing, then it draws every move. */
function Studio({ onMade }: { onMade: (id: string) => void }) {
  const [codex, setCodex] = useState<boolean | null>(null);
  const [name, setName] = useState("");
  const [desc, setDesc] = useState("");
  const [refs, setRefs] = useState<string[]>([]);
  const [project, setProject] = useState<Project | null>(null);
  const [master, setMaster] = useState<string | null>(null);
  const [prog, setProg] = useState<Progress | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<{ codex: boolean }>("studio_status").then((s) => setCodex(s.codex)).catch(() => setCodex(false));
    const un = listen<Progress>("studio-progress", (e) => setProg(e.payload));
    return () => void un.then((f) => f());
  }, []);

  const pickRefs = async () => {
    const r = await open({ multiple: true, filters: [{ name: "Pictures", extensions: ["png", "jpg", "jpeg", "webp"] }] });
    if (r) setRefs([...refs, ...(Array.isArray(r) ? r : [r])].slice(0, 4));
  };
  const draw = async () => {
    setError(null); setBusy(true); setMaster(null);
    try {
      const p = await invoke<Project>("studio_create", { name, description: desc, refs });
      setProject(p);
      const path = await invoke<string>("studio_master", { id: p.id });
      setMaster(`${convertFileSrc(path)}?t=${Date.now()}`);
    } catch (e) { setError(String(e)); }
    setBusy(false);
  };
  const moves = async () => {
    if (!project) return;
    setError(null); setBusy(true);
    try { onMade(await invoke<string>("studio_moves", { id: project.id })); setProject(null); setMaster(null); setName(""); setDesc(""); setRefs([]); }
    catch (e) { setError(String(e)); }
    setBusy(false);
  };

  if (codex === false) {
    return (
      <div className="studio">
        <p className="hint">Character Studio draws with the free <b>Codex CLI</b> and your ChatGPT sign-in. Install it once, then come back:</p>
        <pre className="code">npm i -g @openai/codex{"\n"}codex</pre>
      </div>
    );
  }
  const step = prog?.step;
  return (
    <div className="studio">
      {!master && (
        <>
          <div className="studio-fields">
            <input className="input" placeholder="Name it, e.g. Grumpy Frog" value={name} onChange={(e) => setName(e.target.value)} disabled={busy} />
            <textarea className="input" rows={3} placeholder="Describe it: what it is, how it looks, its personality, the drawing style. e.g. a tired old frog in a cardigan, scratchy pencil sketch style, always looks unimpressed" value={desc} onChange={(e) => setDesc(e.target.value)} disabled={busy} />
            <div className="studio-refs">
              {refs.map((r) => <span key={r} className="chip">{r.split(/[\\/]/).pop()}<button onClick={() => setRefs(refs.filter((x) => x !== r))}>×</button></span>)}
              <button className="btn ghost sm" onClick={pickRefs} disabled={busy}>+ Pictures of it (optional)</button>
            </div>
          </div>
          <button className="btn primary" disabled={busy || !name.trim() || desc.trim().length < 8} onClick={draw}>{busy ? "Drawing… (about 3 minutes)" : "Draw it"}</button>
        </>
      )}
      {master && (
        <div className="studio-master">
          <div className="studio-pic"><img src={master} alt={name} /></div>
          <div className="studio-side">
            <b>Is this {name}?</b>
            <span className="hint">Next it draws 7 sets of moves (talking, thinking, sending, sleeping…) in this exact look. That takes about 15 minutes; you can keep using your PC.</span>
            {!busy && (
              <div className="studio-acts">
                <button className="btn primary" onClick={moves}>Looks good, make the moves</button>
                <button className="btn ghost" onClick={draw}>Draw it again</button>
              </div>
            )}
          </div>
        </div>
      )}
      {busy && prog && (step === "sheets" || step === "pack" || step === "master") && (
        <div className="studio-prog">
          <div className="bar-prog"><i style={{ width: `${step === "master" ? 30 : ((prog.done + (step === "pack" ? 1 : 0.4)) / (prog.total + 1)) * 100}%` }} /></div>
          <span className="hint">{prog.message}{step === "sheets" ? ` · ${prog.done} of ${prog.total}` : ""}</span>
        </div>
      )}
      {error && <div className="banner warn">{error}</div>}
    </div>
  );
}

export default function Buddy({ settings, save }: PageProps) {
  const [chars, setChars] = useState<CharacterInfo[]>([]);
  const [msg, setMsg] = useState<string | null>(null);
  const set = <K extends keyof Settings>(k: K, v: Settings[K]) => save({ ...settings, [k]: v });
  const load = () => allCharacters().then(setChars);
  useEffect(() => {
    load();
    const un = listen("characters-changed", load);
    return () => void un.then((f) => f());
  }, []);
  const buddy = settings.overlay_style === "buddy";

  const importPack = async () => {
    const f = await open({ multiple: false, filters: [{ name: "Baa character", extensions: ["baa", "zip"] }] });
    if (!f) return;
    try { const id = await invoke<string>("character_import", { path: f }); set("character", id); setMsg(null); }
    catch (e) { setMsg(String(e)); }
  };

  return (
    <>
      <div className="page-head">
        <h1>Buddy</h1>
        <p>What shows up while you talk: the little bar, or a character that says your words in a speech bubble.</p>
      </div>

      <div className="style-pick">
        <button className={`pick-card ${!buddy ? "on" : ""}`} onClick={() => set("overlay_style", "bar")}>
          <div className="pick-stage"><div className="dbar mini"><div className="face-wrap"><Face mood="listening" size={18} extras={false} /></div><span className="label">Listening…</span></div></div>
          <b>The bar</b><span>A small capsule at the bottom of the screen</span>
        </button>
        <button className={`pick-card ${buddy ? "on" : ""}`} onClick={() => set("overlay_style", "buddy")}>
          <div className="pick-stage buddy-mini"><span className="mini-bubble">hey, can you…</span><Face mood="listening" size={26} extras={false} /></div>
          <b>A buddy</b><span>A character that mouths along and talks in a bubble</span>
        </button>
      </div>

      {buddy && (
        <>
          <div className="section">
            <h2>Character</h2>
            <div className="char-grid">
              {chars.map((c) => (
                <CharCard key={c.id} info={c} on={settings.character === c.id} onPick={() => set("character", c.id)}
                  onRemove={c.dir ? () => invoke("character_delete", { id: c.id }).then(load) : undefined} />
              ))}
              <button className="char-card add" onClick={importPack}><div className="char-stage">+</div><b>Add from file</b><span>A .baa character</span></button>
            </div>
            {msg && <div className="banner warn">{msg}</div>}
          </div>

          <div className="section">
            <h2>Where it lives</h2>
            <div className="rows" style={{ marginTop: 12 }}>
              <Row title="Place" desc="Drag it to move it. Double-click it to open Baa.">
                <Select width={250} value={settings.buddy_place} onChange={(v) => set("buddy_place", v)} options={[
                  { value: "taskbar", label: "Sitting on the taskbar" },
                  { value: "popup", label: "Only while I'm talking" },
                  { value: "free", label: "Anywhere I put it" },
                ]} />
              </Row>
              <Row title="Size">
                <Select width={250} value={settings.buddy_size} onChange={(v) => set("buddy_size", v)} options={[
                  { value: "small", label: "Small" }, { value: "medium", label: "Medium" }, { value: "large", label: "Large" },
                ]} />
              </Row>
              <Row title="Dozes off" desc="Falls asleep after a minute and a half with nothing to do, and wakes up when you talk.">
                <Toggle on={settings.buddy_sleeps} onChange={(v) => set("buddy_sleeps", v)} />
              </Row>
            </div>
          </div>
        </>
      )}

      <div className="section">
        <h2>Make your own</h2>
        <p className="lead">Describe any character and Baa draws it with every move it needs: talking, thinking, sending, dozing off. Based on Sprite Studio by John Kinyanjui.</p>
        <Studio onMade={(id) => { load(); save({ ...settings, overlay_style: "buddy", character: id }); }} />
      </div>
    </>
  );
}
