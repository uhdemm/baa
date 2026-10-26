// Buddy mode: the character with a speech bubble over its head. It mouths along
// while you talk (the bubble fills with your words), chews while it thinks, throws
// when it sends, and dozes off when nothing's happening. Grab it to move it;
// double-click it to open Baa.
import React, { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import ReactDOM from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Character, duration, findCharacter, loadCharacter, Mood, stateFor } from "./character";
import Sprite, { headOf } from "./ui/Sprite";
import { applyAppearance, type Appearance } from "./appearance";
import "./styles.css";

document.documentElement.classList.add("overlay", "buddy-page");

type State = "idle" | "listen" | "think" | "send" | "nothing" | "error" | "cancel";
interface OverlayEvent { state: State; message: string | null; kind: "dictate" | "command" }
interface BuddySettings { character: string; buddy_size: string; buddy_place: string; buddy_sleeps: boolean; buddy_jiggle?: boolean; appearance: Appearance }

// full-body height on screen, px (medium = about the size of a taskbar icon stack)
const SIZE: Record<string, number> = { small: 56, medium: 76, large: 110 };
const LEFT = 14; // the character stands at the window's left; the bubble opens to its right
const SLEEP_AFTER = 90_000;
// demo in a plain browser: buddy.html?mood=listen&text=Hello+there
const demo = import.meta.env.DEV && !(window as unknown as Record<string, unknown>).__TAURI_INTERNALS__ ? new URLSearchParams(location.search) : null;

function Buddy() {
  const [s, setS] = useState<BuddySettings | null>(null);
  const [c, setC] = useState<Character | null>(null);
  const [mood, setMood] = useState<Mood>((demo?.get("mood") as Mood) || "idle");
  const [bubble, setBubble] = useState<string | null>(demo?.get("text") ?? null);
  const [level, setLevel] = useState(demo ? 0.5 : 0);
  const smooth = useRef(0);
  const moodRef = useRef<Mood>("idle");
  const after = useRef<number | undefined>(undefined);
  const sleepTimer = useRef<number | undefined>(undefined);
  const bubbleTimer = useRef<number | undefined>(undefined);
  const spriteEl = useRef<HTMLDivElement | null>(null);
  const dragging = useRef(false);
  const kind = useRef<"dictate" | "command">("dictate");

  // settings + character
  useEffect(() => {
    const apply = (v: BuddySettings) => {
      setS(v);
      if (v.appearance) applyAppearance(v.appearance);
      // demo: buddy.html?char=<id> plays a pack served from /characters/<id> (for checking a new character)
      (demo?.get("char") ? loadCharacter(demo.get("char")!) : findCharacter(v.character)).then(setC).catch(() => findCharacter("sheep").then(setC));
    };
    if (demo) { apply({ character: "sheep", buddy_size: "medium", buddy_place: "taskbar", buddy_sleeps: true, buddy_jiggle: true, appearance: undefined as unknown as Appearance }); return; }
    invoke<BuddySettings>("get_settings").then(apply);
    const a = listen<BuddySettings>("buddy-changed", (e) => apply(e.payload));
    const b = listen<Appearance>("appearance-changed", (e) => applyAppearance(e.payload));
    const d = listen("characters-changed", () => invoke<BuddySettings>("get_settings").then(apply));
    return () => { a.then((f) => f()); b.then((f) => f()); d.then((f) => f()); };
  }, []);

  /** Play a mood; a "once" mood hands over to `then` when it finishes. */
  const play = useCallback((m: Mood, then?: Mood) => {
    window.clearTimeout(after.current);
    moodRef.current = m;
    setMood(m);
    if (then && c) after.current = window.setTimeout(() => play(then), Math.min(duration(stateFor(c, m)), 4000) + 250);
  }, [c]);

  // nothing happening for a while: doze off
  const restartSleep = useCallback(() => {
    window.clearTimeout(sleepTimer.current);
    if (s?.buddy_sleeps && s.buddy_place !== "popup") {
      sleepTimer.current = window.setTimeout(() => { if (moodRef.current === "idle") play("sleep"); }, SLEEP_AFTER);
    }
  }, [s, play]);
  useEffect(() => { restartSleep(); }, [restartSleep]);

  const say = (text: string | null, holdMs?: number) => {
    window.clearTimeout(bubbleTimer.current);
    setBubble(text);
    if (holdMs) bubbleTimer.current = window.setTimeout(() => setBubble(null), holdMs);
  };

  useEffect(() => {
    if (demo || !c) return;
    const a = listen<OverlayEvent>("overlay-state", (e) => {
      const { state, message } = e.payload;
      kind.current = e.payload.kind;
      restartSleep();
      const cur = moodRef.current;
      switch (state) {
        case "listen":
          say(kind.current === "command" ? "Say what to change…" : "");
          // waking up or popping up first, then straight into listening
          if (cur === "sleep") play("wake", "listen");
          else if (cur === "idle" && s?.buddy_place === "popup") play("appear", "listen");
          else play("listen");
          break;
        case "think": say(message ?? "Hmm…"); play("think"); break;
        case "send": play("send", "idle"); break;
        case "nothing": say("Didn't catch that", 1800); play("nothing", "idle"); break;
        case "error": say(message ?? "Something went wrong", 3200); play("error", "idle"); break;
        case "cancel": say("Never mind", 900); play("cancel", "idle"); break;
        case "idle": if (cur !== "send" && cur !== "sleep") play("idle"); break;
      }
    });
    const b = listen<number>("mic-level", (e) => { smooth.current = smooth.current * 0.5 + e.payload * 0.5; setLevel(smooth.current); });
    const d = listen<string>("live-text", (e) => { if (moodRef.current === "listen" || moodRef.current === "appear" || moodRef.current === "wake") say(e.payload); });
    const f = listen<string>("dictation-done", (e) => say(e.payload, 3500)); // what actually got pasted
    return () => { a.then((x) => x()); b.then((x) => x()); d.then((x) => x()); f.then((x) => x()); };
  }, [c, s, play, restartSleep]);

  // tell the window which box is the character, so only it catches the mouse
  const height = SIZE[s?.buddy_size ?? "medium"] ?? 76;
  useLayoutEffect(() => {
    if (demo) return;
    const el = spriteEl.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    invoke("buddy_hit", { x: r.left, y: r.top, w: r.width, h: r.height }).catch(() => {});
  }, [c, height]);

  // drag to move; remember where once it settles
  useEffect(() => {
    if (demo) return;
    let t: number | undefined;
    const un = getCurrentWindow().onMoved(() => {
      if (!dragging.current) return;
      window.clearTimeout(t);
      t = window.setTimeout(() => { dragging.current = false; invoke("buddy_moved").catch(() => {}); }, 350);
    });
    return () => void un.then((f) => f());
  }, []);

  if (!c) return null;
  const [hx, hy] = headOf(c, mood);
  const w = (c.slot.w / c.slot.h) * height;
  const showBubble = bubble !== null && (bubble !== "" || mood === "listen");
  const text = bubble && bubble.length > 120 ? "…" + bubble.slice(-120) : bubble;

  return (
    <div className="buddy">
      {showBubble && (
        <div className="bubble" style={{ left: LEFT + w * hx + 6, bottom: height * (1 - hy) - 6 }}>
          {text ? <span>{text}</span> : <span className="dots"><i /><i /><i /></span>}
        </div>
      )}
      <div
        className="buddy-sprite"
        style={{ width: w, height, left: LEFT }}
        onMouseDown={(e) => { if (e.button === 0 && !demo) { dragging.current = true; getCurrentWindow().startDragging().catch(() => {}); } }}
        onDoubleClick={() => !demo && invoke("open_baa", { page: null })}
        title="Drag to move · double-click to open Baa"
      >
        <Sprite character={c} mood={mood} level={level} height={height} jiggle={s?.buddy_jiggle ?? true} onRef={(el) => { spriteEl.current = el; }} />
      </div>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Buddy />
  </React.StrictMode>,
);
