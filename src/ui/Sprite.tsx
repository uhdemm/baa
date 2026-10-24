import { useEffect, useRef, useState } from "react";
import { Character, frameAt, Mood, stateFor } from "../character";

/**
 * Plays a character. `height` is the drawn height in px; the frame's flat bottom
 * edge sits on the element's bottom. `level` (0..1) drives the mouth while listening.
 */
export default function Sprite({ character: c, mood, level = 0, height, className, onRef }: {
  character: Character; mood: Mood; level?: number; height: number; className?: string;
  onRef?: (el: HTMLDivElement | null) => void;
}) {
  const st = stateFor(c, mood);
  const sheet = c.sheets[st.sheet];
  const started = useRef(performance.now());
  const [, tick] = useState(0);
  const wobble = useRef(Math.random());

  useEffect(() => { started.current = performance.now(); }, [mood, c.id]);
  // redraw on the mood's own beat (mouth: ~14 times a second, following the mic)
  useEffect(() => {
    const ms = st.mode === "mouth" ? 70 : 1000 / (st.fps ?? 8);
    const id = window.setInterval(() => {
      if (st.mode === "mouth") wobble.current = Math.random();
      tick((n) => n + 1);
    }, ms);
    return () => window.clearInterval(id);
  }, [st]);

  const f = frameAt(st, performance.now() - started.current, level, wobble.current);
  const col = f % sheet.cols, row = Math.floor(f / sheet.cols);
  const w = (c.slot.w / c.slot.h) * height;
  return (
    <div
      ref={onRef}
      className={className}
      role="img"
      aria-label={c.name}
      style={{
        width: w,
        height,
        backgroundImage: `url("${c.url(sheet.file)}")`,
        backgroundSize: `${sheet.cols * 100}% ${sheet.rows * 100}%`,
        backgroundPosition: `${sheet.cols > 1 ? (col / (sheet.cols - 1)) * 100 : 0}% ${sheet.rows > 1 ? (row / (sheet.rows - 1)) * 100 : 0}%`,
        backgroundRepeat: "no-repeat",
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
