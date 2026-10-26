import { useEffect, useRef, useState } from "react";
import { CharState, Character, frameAt, Mood, stateFor } from "../character";

const cycleMs = (st: CharState) => (((st.intro?.length ?? 0) + st.frames.length) / (st.fps ?? 8)) * 1000;

/**
 * Plays a character. `height` is the drawn height in px; the frame's flat bottom
 * edge sits on the element's bottom. `level` (0..1) drives the mouth while listening.
 * A looping mood with `variants` breaks into one of them now and then (never the same
 * one twice running). `jiggle` adds spring physics: it squashes and stretches whenever
 * the pose changes, lands, or talks, and wobbles back into shape.
 */
export default function Sprite({ character: c, mood, level = 0, height, className, onRef, jiggle = false }: {
  character: Character; mood: Mood; level?: number; height: number; className?: string;
  onRef?: (el: HTMLDivElement | null) => void; jiggle?: boolean;
}) {
  const base = stateFor(c, mood);
  // what's playing right now: the mood itself, or one of its variants
  const [play, setPlay] = useState<CharState>(base);
  const started = useRef(performance.now());
  const [, tick] = useState(0);
  const wobble = useRef(Math.random());
  const lastVariant = useRef(-1);

  useEffect(() => { started.current = performance.now(); setPlay(base); }, [mood, c.id]); // eslint-disable-line react-hooks/exhaustive-deps
  // redraw on the mood's own beat (mouth: ~14 times a second, following the mic)
  useEffect(() => {
    const ms = play.mode === "mouth" ? 70 : 1000 / (play.fps ?? 8);
    const id = window.setInterval(() => {
      if (play.mode === "mouth") wobble.current = Math.random();
      const t = performance.now() - started.current;
      if (play !== base && t >= cycleMs(play)) {
        // a variant finished: back to the loop
        started.current = performance.now();
        setPlay(base);
      } else if (play === base && base.mode === "loop" && base.variants?.length && t >= cycleMs(base)) {
        started.current = performance.now();
        if (Math.random() < 0.6) {
          let k = Math.floor(Math.random() * base.variants.length);
          if (k === lastVariant.current) k = (k + 1) % base.variants.length;
          lastVariant.current = k;
          setPlay(base.variants[k]);
        }
      }
      tick((n) => n + 1);
    }, ms);
    return () => window.clearInterval(id);
  }, [play, base]);

  const st = play;
  const sheet = c.sheets[st.sheet] ?? c.sheets[base.sheet];
  const f = Math.min(frameAt(st, performance.now() - started.current, level, wobble.current), sheet.frames - 1);

  // ---- jiggle: a damped spring on stretch (s) and lean (r), kicked by pose changes ----
  const el = useRef<HTMLDivElement | null>(null);
  const spring = useRef({ s: 0, v: 0, r: 0, rv: 0, raf: 0, last: 0 });
  const kick = (dv: number, drv = 0) => {
    if (!jiggle || !el.current) return;
    const sp = spring.current;
    sp.v += dv; sp.rv += drv;
    // already springing; unless the window was hidden and its frames stalled, then start over
    if (sp.raf && performance.now() - sp.last < 250) return;
    cancelAnimationFrame(sp.raf);
    sp.last = performance.now();
    const step = () => {
      const now = performance.now();
      const dt = Math.min(0.032, (now - sp.last) / 1000);
      sp.last = now;
      // stiff and a bit underdamped, so it overshoots and wobbles back a couple of times
      sp.v += (-260 * sp.s - 11 * sp.v) * dt; sp.s += sp.v * dt;
      sp.rv += (-200 * sp.r - 9 * sp.rv) * dt; sp.r += sp.rv * dt;
      if (el.current) el.current.style.transform = `rotate(${sp.r}deg) scale(${1 - sp.s * 0.55}, ${1 + sp.s})`;
      if (Math.abs(sp.s) + Math.abs(sp.v) * 0.05 + Math.abs(sp.r) * 0.02 + Math.abs(sp.rv) * 0.002 < 0.0008) {
        sp.s = sp.v = sp.r = sp.rv = 0; sp.raf = 0;
        if (el.current) el.current.style.transform = "";
        return;
      }
      sp.raf = requestAnimationFrame(step);
    };
    sp.raf = requestAnimationFrame(step);
    // frames don't run while the window is hidden: settle back to normal shape instead of freezing mid-squash
    window.setTimeout(() => {
      if (sp.raf && performance.now() - sp.last > 400) {
        cancelAnimationFrame(sp.raf);
        sp.s = sp.v = sp.r = sp.rv = 0; sp.raf = 0;
        if (el.current) el.current.style.transform = "";
      }
    }, 500);
  };
  useEffect(() => () => cancelAnimationFrame(spring.current.raf), []);
  useEffect(() => { if (!jiggle && el.current) el.current.style.transform = ""; }, [jiggle]);
  // a new pose: a quick squash and a little lean to a random side
  const lastFrame = useRef(`${st.sheet}:${f}`);
  const key = `${st.sheet}:${f}`;
  useEffect(() => {
    if (key === lastFrame.current) return;
    lastFrame.current = key;
    if (st.mode === "mouth") return;
    kick(-1.6 - Math.random() * 0.8, (Math.random() - 0.5) * 70);
  }, [key]); // eslint-disable-line react-hooks/exhaustive-deps
  // a new mood (appearing, sending, waking) is a bigger bounce
  useEffect(() => { kick(-3.2, (Math.random() - 0.5) * 90); }, [mood]); // eslint-disable-line react-hooks/exhaustive-deps
  // talking: every time the voice gets louder it bobs
  const prevLevel = useRef(0);
  useEffect(() => {
    const rise = level - prevLevel.current;
    prevLevel.current = level;
    if (st.mode === "mouth" && rise > 0.08) kick(rise * 3.5, (Math.random() - 0.5) * 20);
  }, [level]); // eslint-disable-line react-hooks/exhaustive-deps

  const col = f % sheet.cols, row = Math.floor(f / sheet.cols);
  const w = (c.slot.w / c.slot.h) * height;
  return (
    <div
      ref={(n) => { el.current = n; onRef?.(n); }}
      className={className}
      role="img"
      aria-label={c.name}
      style={{
        width: w,
        height,
        transformOrigin: "50% 100%",
        backgroundImage: `url("${c.url(sheet.file)}")`,
        backgroundSize: `${sheet.cols * 100}% ${sheet.rows * 100}%`,
        backgroundPosition: `${sheet.cols > 1 ? (col / (sheet.cols - 1)) * 100 : 0}% ${sheet.rows > 1 ? (row / (sheet.rows - 1)) * 100 : 0}%`,
        backgroundRepeat: "no-repeat",
        imageRendering: c.pixel ? "pixelated" : undefined,
      }}
    />
  );
}

/** Where the top of the head is in the current frame, as a fraction of the slot (for the bubble's tail). */
export function headOf(c: Character, mood: Mood): [number, number] {
  const st = stateFor(c, mood);
  const h = c.sheets[st.sheet].heads[st.frames[0]] ?? [c.slot.w / 2, 0];
  return [h[0] / c.slot.w, h[1] / c.slot.h];
}
