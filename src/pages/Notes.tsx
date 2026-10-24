import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, MeetingStatus, Note } from "../api";
import type { PageProps } from "../App";
import Face from "../ui/Face";
import { Icon } from "../ui/controls";

const when = (ts: number) =>
  new Date(ts * 1000).toLocaleString(undefined, { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" });
const clock = (s: number) => `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;

/** Notes you type or dictate into, plus meeting notes Baa writes for you. */
export default function Notes({ settings }: PageProps) {
  const [notes, setNotes] = useState<Note[]>([]);
  const [query, setQuery] = useState("");
  const [open, setOpen] = useState<Note | null>(null);
  const [meeting, setMeeting] = useState<MeetingStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const timer = useRef<number | undefined>(undefined);
  const draft = useRef<Note | null>(null);

  const load = useCallback(() => api.notesList(query).then(setNotes), [query]);
  useEffect(() => { load(); }, [load]);

  // meeting state: events while it runs, plus a 1 s tick for the clock
  useEffect(() => {
    const poll = () => api.meetingStatus().then(setMeeting);
    poll();
    const tick = window.setInterval(poll, 1000);
    const a = listen("meeting-changed", poll);
    const b = listen<number | null>("meeting-done", async (e) => {
      const list = await api.notesList();
      setNotes(list);
      const made = list.find((n) => n.id === e.payload);
      if (made) setOpen(made);
    });
    return () => { window.clearInterval(tick); a.then((f) => f()); b.then((f) => f()); };
  }, []);

  // autosave half a second after typing stops
  const edit = (patch: Partial<Note>) => {
    const next = { ...(draft.current ?? open!), ...patch };
    draft.current = next;
    setOpen(next);
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(flush, 500);
  };
  const flush = async () => {
    const n = draft.current;
    draft.current = null;
    if (!n) return;
    const id = await api.noteSave(n.id > 0 ? n.id : null, n.title, n.body);
    if (n.id <= 0) setOpen((o) => (o && o.id === n.id ? { ...o, id } : o));
    load();
  };
  const select = async (n: Note | null) => { await flush(); setOpen(n); };
  const create = () => select({ id: -Date.now(), created: Date.now() / 1000, updated: Date.now() / 1000, title: "", body: "", kind: "note" });
  const remove = async (n: Note) => {
    window.clearTimeout(timer.current);
    draft.current = null;
    if (n.id > 0) await api.noteDelete(n.id);
    setOpen(null);
    load();
  };

  const startMeeting = async () => {
    setError(null);
    try { await api.meetingStart(); setMeeting(await api.meetingStatus()); } catch (e) { setError(String(e)); }
  };
  const live = meeting?.running || meeting?.finishing;
  const [aiOn, setAiOn] = useState(true);
  useEffect(() => { api.aiStatus().then((s) => setAiOn(s.ready)); }, []);

  return (
    <>
      <div className="page-head">
        <h1>Notes</h1>
        <p>Jot things down by voice, or let Baa listen to a call and write the notes for you.</p>
      </div>

      <div className={`meet-card ${live ? "live" : ""}`}>
        <div className="face-wrap"><Face size={26} mood={meeting?.finishing ? "thinking" : meeting?.running ? "listening" : "idle"} extras={false} /></div>
        <div className="meet-text">
          {meeting?.running ? (
            <>
              <b>Listening to your meeting · {clock(meeting.seconds)}</b>
              <span>{meeting.hears_call ? "Hearing you and the call. Words show up every 20 seconds." : "Only your mic: Baa can't hear the call audio on this PC."}</span>
            </>
          ) : meeting?.finishing ? (
            <>
              <b>Writing your notes…</b>
              <span>Summary, decisions and action items, then the full transcript.</span>
            </>
          ) : (
            <>
              <b>Meeting notes</b>
              <span>Records your mic and the call (Zoom, Meet, Teams, Discord), then writes a summary and action items{aiOn ? "" : " (set up AI for the summary)"}.</span>
            </>
          )}
        </div>
        {meeting?.running ? (
          <button className="btn danger" onClick={() => api.meetingStop()}><i className="rec-dot" /> Stop</button>
        ) : (
          <button className="btn primary" disabled={!!meeting?.finishing} onClick={startMeeting}><i className="rec-dot" /> Start</button>
        )}
      </div>
      {error && <div className="banner warn">{error}</div>}
      {meeting?.running && meeting.transcript && <pre className="meet-live">{meeting.transcript.split("\n").slice(-6).join("\n")}</pre>}

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
              <textarea className="note-body" placeholder={`Start typing, or hold ${settings.shortcut} and talk.`} value={open.body} onChange={(e) => edit({ body: e.target.value })} />
            </>
          ) : (
            <div className="empty center">Pick a note, or press New. Dictation works right in here.</div>
          )}
        </div>
      </div>
    </>
  );
}
