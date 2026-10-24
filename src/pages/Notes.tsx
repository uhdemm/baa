import { useCallback, useEffect, useRef, useState } from "react";
import { emit, listen } from "@tauri-apps/api/event";
import { api, MeetingStatus, Note, Template } from "../api";
import type { PageProps } from "../App";
import Face from "../ui/Face";
import { Icon, Select } from "../ui/controls";

const when = (ts: number) =>
  new Date(ts * 1000).toLocaleString(undefined, { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" });
const clock = (s: number) => `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;

const TEMPLATES: { value: Template; label: string }[] = [
  { value: "general", label: "General meeting" },
  { value: "one_on_one", label: "1:1" },
  { value: "sales", label: "Sales call" },
  { value: "standup", label: "Standup" },
  { value: "interview", label: "Interview" },
];
const ASKS = ["Draft a follow-up email", "What did I agree to do?", "Summarize in 3 bullets", "Write a Slack update"];

/** Speaker labels still in a meeting transcript ("Speaker 2", "Them"), in order of appearance. */
const speakerLabels = (body: string) => [...new Set([...body.matchAll(/\] (Speaker \d+|Them):/g)].map((m) => m[1]))];

function Rename({ label, onRename }: { label: string; onRename: (name: string) => void }) {
  const [v, setV] = useState("");
  const done = () => { if (v.trim()) onRename(v.trim()); setV(""); };
  return (
    <label className="rename">
      <span>{label}</span>
      <input className="input" placeholder="Name" value={v} onChange={(e) => setV(e.target.value)} onBlur={done} onKeyDown={(e) => e.key === "Enter" && done()} />
    </label>
  );
}

/** Notes you type or dictate into, plus meeting notes Baa writes for you. */
export default function Notes({ settings }: PageProps) {
  const [notes, setNotes] = useState<Note[]>([]);
  const [query, setQuery] = useState("");
  const [open, setOpen] = useState<Note | null>(null);
  const [meeting, setMeeting] = useState<MeetingStatus | null>(null);
  const [setup, setSetup] = useState({ attendees: "", my_notes: "", template: "general" as Template });
  const [error, setError] = useState<string | null>(null);
  const [aiOn, setAiOn] = useState(true);
  const [ask, setAsk] = useState("");
  const [answer, setAnswer] = useState<{ q: string; a: string | null } | null>(null);
  const timer = useRef<number | undefined>(undefined);
  const setupTimer = useRef<number | undefined>(undefined);
  const draft = useRef<Note | null>(null);
  const seeded = useRef(false);

  const load = useCallback(() => api.notesList(query).then(setNotes), [query]);
  useEffect(() => { load(); }, [load]);
  useEffect(() => { api.aiStatus().then((s) => setAiOn(s.ready)); }, []);

  // meeting state: events while it runs, plus a 1 s tick for the clock
  useEffect(() => {
    const poll = () => api.meetingStatus().then((m) => {
      setMeeting(m);
      if (!seeded.current) {
        seeded.current = true;
        setSetup({ attendees: m.attendees, my_notes: m.my_notes, template: m.template || "general" });
      }
    });
    poll();
    const tick = window.setInterval(poll, 1000);
    const a = listen("meeting-changed", poll);
    const b = listen<number | null>("meeting-done", async (e) => {
      const list = await api.notesList();
      setNotes(list);
      const made = list.find((n) => n.id === e.payload);
      if (made) setOpen(made);
      setSetup((s) => ({ ...s, my_notes: "" }));
    });
    return () => { window.clearInterval(tick); a.then((f) => f()); b.then((f) => f()); };
  }, []);

  const changeSetup = (patch: Partial<typeof setup>) => {
    const next = { ...setup, ...patch };
    setSetup(next);
    window.clearTimeout(setupTimer.current);
    setupTimer.current = window.setTimeout(() => api.meetingUpdate(next.attendees, next.my_notes, next.template), 400);
  };

  // autosave half a second after typing stops
  const flush = async () => {
    const n = draft.current;
    draft.current = null;
    if (!n) return;
    const id = await api.noteSave(n.id > 0 ? n.id : null, n.title, n.body);
    if (n.id <= 0) setOpen((o) => (o && o.id === n.id ? { ...o, id } : o));
    load();
  };
  const edit = (patch: Partial<Note>) => {
    const next = { ...(draft.current ?? open!), ...patch };
    draft.current = next;
    setOpen(next);
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(flush, 500);
  };
  const select = async (n: Note | null) => { await flush(); setOpen(n); setAnswer(null); };
  const create = () => select({ id: -Date.now(), created: Date.now() / 1000, updated: Date.now() / 1000, title: "", body: "", kind: "note" });
  const remove = async (n: Note) => {
    window.clearTimeout(timer.current);
    draft.current = null;
    if (n.id > 0) await api.noteDelete(n.id);
    setOpen(null);
    load();
  };
  const rename = (label: string, name: string) => {
    if (!open) return;
    const swap = (t: string) => t.split(label).join(name);
    edit({ title: swap(open.title), body: swap(open.body) });
  };
  const askNote = async (q: string) => {
    if (!open || !q.trim()) return;
    setAnswer({ q, a: null });
    setAsk("");
    try { setAnswer({ q, a: await api.noteAsk(open.title, open.body, q) }); } catch (e) { setAnswer({ q, a: String(e) }); }
  };

  const startMeeting = async () => {
    setError(null);
    try {
      await api.meetingUpdate(setup.attendees, setup.my_notes, setup.template);
      await api.meetingStart();
      setMeeting(await api.meetingStatus());
    } catch (e) { setError(String(e)); }
  };
  const running = !!meeting?.running;
  const live = running || meeting?.finishing;
  const labels = open?.kind === "meeting" ? speakerLabels(open.body) : [];

  return (
    <>
      <div className="page-head">
        <h1>Notes</h1>
        <p>Jot things down by voice, or let Baa listen to a call and write the notes for you.</p>
      </div>

      <div className={`meet ${live ? "live" : ""}`}>
        <div className="meet-card">
          <div className="face-wrap"><Face size={26} mood={meeting?.finishing ? "thinking" : running ? "listening" : "idle"} extras={false} /></div>
          <div className="meet-text">
            {running ? (
              <>
                <b>Listening to your meeting · {clock(meeting!.seconds)}</b>
                <span>{meeting!.hears_call ? "Hearing you and the call. Your mic is cleaned of the call's echo, so only you count as Me." : "Only your mic: Baa can't hear the call audio on this PC."}</span>
              </>
            ) : meeting?.finishing ? (
              <>
                <b>{meeting.stage || "Writing your notes…"}</b>
                <span>{meeting.speakers ? "Telling people apart, then writing the summary and action items." : "Writing the summary and action items."}</span>
              </>
            ) : (
              <>
                <b>Meeting notes</b>
                <span>Records you and the call (Zoom, Meet, Teams, Discord), tells people apart, then writes the notes{aiOn ? "" : " (set up AI for the summary)"}.</span>
              </>
            )}
          </div>
          {running ? (
            <button className="btn danger" onClick={() => api.meetingStop()}><i className="rec-dot" /> Stop</button>
          ) : (
            <button className="btn primary" disabled={!!meeting?.finishing} onClick={startMeeting}><i className="rec-dot" /> Start</button>
          )}
        </div>
        {!meeting?.finishing && (
          <div className="meet-setup">
            <input className="input" placeholder="Who's on the call? e.g. Brad, Sarah from HotBox" value={setup.attendees} onChange={(e) => changeSetup({ attendees: e.target.value })} />
            <Select width={170} value={setup.template} onChange={(v) => changeSetup({ template: v as Template })} options={TEMPLATES} />
          </div>
        )}
        {meeting && !meeting.speakers && !live && (
          <p className="hint meet-hint">
            To tell the other people apart, get <b>Speaker detection</b> (139 MB).{" "}
            <button className="link" onClick={() => emit("navigate", "models")}>Open Models</button>
          </p>
        )}
        {running && (
          <div className="meet-live-grid">
            <textarea className="input meet-mine" placeholder="Your own notes. Type key points as you go: the AI builds the notes around them." value={setup.my_notes} onChange={(e) => changeSetup({ my_notes: e.target.value })} />
            <pre className="meet-live">{meeting!.transcript ? meeting!.transcript.split("\n").slice(-8).join("\n") : "Words show up here every 20 seconds."}</pre>
          </div>
        )}
      </div>
      {error && <div className="banner warn">{error}</div>}

      <div className="notes">
        <div className="notes-list">
          <div className="searchbar">
            <input className="input" style={{ flex: 1 }} placeholder="Search notes" value={query} onChange={(e) => setQuery(e.target.value)} />
            <button className="btn" onClick={create}>New</button>
          </div>
          {notes.length === 0 && <div className="empty"><Face size={22} mood="thinking" /> No notes yet.</div>}
          <ul>
            {notes.map((n) => (
              <li key={n.id}>
                <button className={`note-row ${open?.id === n.id ? "on" : ""}`} onClick={() => select(n)}>
                  <b>{n.kind === "meeting" && <Icon name="mic" size={12} />}{n.title || "Untitled"}</b>
                  <span>{when(n.updated)} · {n.body.replace(/\s+/g, " ").slice(0, 60) || "Empty"}</span>
                </button>
              </li>
            ))}
          </ul>
        </div>
        <div className="note-edit">
          {open ? (
            <>
              <div className="note-tools">
                <span className="hint">{open.kind === "meeting" ? "Meeting · " : ""}{when(open.created)}</span>
                <button className="btn ghost sm" onClick={() => navigator.clipboard.writeText(`${open.title}\n\n${open.body}`)}>Copy</button>
                <button className="btn ghost sm" onClick={() => remove(open)}>Delete</button>
              </div>
              <input className="note-title" placeholder="Title" value={open.title} onChange={(e) => edit({ title: e.target.value })} autoFocus={!open.title} />
              {labels.length > 0 && (
                <div className="renames">
                  <span className="hint">Who's who? Name them and the whole note updates.</span>
                  {labels.map((l) => <Rename key={l} label={l} onRename={(name) => rename(l, name)} />)}
                </div>
              )}
              <textarea className="note-body" placeholder={`Start typing, or hold ${settings.shortcut} and talk.`} value={open.body} onChange={(e) => edit({ body: e.target.value })} />
              {aiOn && open.body.trim() && (
                <div className="ask">
                  {answer && (
                    <div className="answer">
                      <div className="answer-q">{answer.q}</div>
                      {answer.a === null ? <div className="hint">Thinking…</div> : <p>{answer.a}</p>}
                      {answer.a && (
                        <div className="answer-acts">
                          <button className="btn ghost sm" onClick={() => navigator.clipboard.writeText(answer.a!)}>Copy</button>
                          <button className="btn ghost sm" onClick={() => { edit({ body: `${open.body.trimEnd()}\n\n${answer.q}\n${answer.a}` }); setAnswer(null); }}>Add to note</button>
                          <button className="btn ghost sm" onClick={() => setAnswer(null)}>Close</button>
                        </div>
                      )}
                    </div>
                  )}
                  <div className="ask-chips">
                    {ASKS.map((q) => <button key={q} className="chip" onClick={() => askNote(q)}>{q}</button>)}
                  </div>
                  <input className="input" placeholder="Ask anything about this note…" value={ask} onChange={(e) => setAsk(e.target.value)} onKeyDown={(e) => e.key === "Enter" && askNote(ask)} />
                </div>
              )}
            </>
          ) : (
            <div className="empty center">Pick a note, or press New. Dictation works right in here.</div>
          )}
        </div>
      </div>
    </>
  );
}
