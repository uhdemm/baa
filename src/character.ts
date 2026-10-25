// Character packs (character.json + sprite sheets): loading, and picking the frame to
// show for a mood at a moment in time. The Sheep ships in /characters/sheep; installed
// packs live in the app data folder and load over Tauri's asset protocol.
import { invoke } from "@tauri-apps/api/core";

export type Mood = "appear" | "idle" | "listen" | "think" | "send" | "nothing" | "error" | "cancel" | "sleep" | "wake";

export interface CharState {
  sheet: string;
  frames: number[];
  /** played once before `frames` start looping (sleep: drowsy, then snoring) */
  intro?: number[];
  /** mouth mode: shapes thrown in now and then so talking doesn't look mechanical */
  extra?: number[];
  fps?: number;
  /** loop: repeat · once: play and hold the last frame · mouth: frame follows the mic */
  mode: "loop" | "once" | "mouth";
}

export interface Sheet { file: string; cols: number; rows: number; frames: number; heads: [number, number][] }

export interface Character {
  format: 1;
  id: string;
  name: string;
  author: string;
  description: string;
  slot: { w: number; h: number };
  belly_width?: number;
  /** pixel art: drawn with hard square pixels, never smoothed */
  pixel?: boolean;
  sheets: Record<string, Sheet>;
  states: Partial<Record<Mood, CharState>>;
  /** filled in on load: where the sheet files are */
  url: (file: string) => string;
}

export interface CharacterInfo { id: string; name: string; author: string; description: string; dir: string | null }

export const BUILTIN: CharacterInfo[] = [
  { id: "sheep", name: "Sheep", author: "Adam", description: "The original. Scruffy, nervous, and he really means it when he hits send.", dir: null },
];

const cache = new Map<string, Promise<Character>>();

/** Loads a character by id: built-in ones from the app, the rest from their install folder. */
export function loadCharacter(id: string, dir: string | null = null): Promise<Character> {
  const key = `${id}|${dir ?? ""}`;
  if (!cache.has(key)) {
    // built-in characters ship with the app; installed ones come from Rust as data: URLs
    // (a file link into the app data folder didn't load)
    const load = dir
      ? invoke<[string, Record<string, string>]>("character_files", { id }).then(([json, files]) => {
          const c = JSON.parse(json) as Character;
          c.url = (f) => files[f] ?? "";
          return c;
        })
      : fetch(`/characters/${id}/character.json`).then(async (r) => {
          if (!r.ok) throw new Error(`Can't load ${id}`);
          const c = (await r.json()) as Character;
          c.url = (f) => `/characters/${id}/${f}`;
          return c;
        });
    cache.set(key, load.then((c) => {
      // warm the sheets so the first animation doesn't flash
      Object.values(c.sheets).forEach((s) => { const i = new Image(); i.src = c.url(s.file); });
      return c;
    }));
    cache.get(key)!.catch(() => cache.delete(key));
  }
  return cache.get(key)!;
}

/** Every character you can pick: built-in plus installed. */
export async function allCharacters(): Promise<CharacterInfo[]> {
  const installed = await invoke<CharacterInfo[]>("characters_list").catch(() => [] as CharacterInfo[]);
  return [...BUILTIN, ...installed];
}

export async function findCharacter(id: string): Promise<Character> {
  const info = (await allCharacters()).find((c) => c.id === id) ?? BUILTIN[0];
  return loadCharacter(info.id, info.dir);
}

/** A mood the pack doesn't have falls back to one it does. */
export function stateFor(c: Character, mood: Mood): CharState {
  const order: Record<Mood, Mood[]> = {
    appear: ["appear", "idle"], idle: ["idle"], listen: ["listen", "idle"], think: ["think", "idle"],
    send: ["send", "idle"], nothing: ["nothing", "idle"], error: ["error", "think", "idle"],
    cancel: ["cancel", "idle"], sleep: ["sleep", "idle"], wake: ["wake", "appear", "idle"],
  };
  for (const m of order[mood]) if (c.states[m]) return c.states[m]!;
  return Object.values(c.states)[0]!;
}

/** How long a "once" mood takes to play, in ms (then it holds its last frame). */
export function duration(st: CharState): number {
  return st.mode === "once" ? (st.frames.length / (st.fps ?? 8)) * 1000 : Infinity;
}

/** The frame to show `t` ms into a mood. `level` (0..1) drives the mouth. */
export function frameAt(st: CharState, t: number, level = 0, wobble = 0): number {
  const fps = st.fps ?? 8;
  if (st.mode === "mouth") {
    const f = st.frames;
    const i = level < 0.1 ? 0 : level < 0.3 ? 1 : level < 0.58 ? 2 : 3;
    // a talking mouth isn't just open/closed: now and then use an "oo" or "ee" shape
    if (i >= 2 && st.extra?.length && wobble < 0.22) return st.extra[Math.floor(wobble * 100) % st.extra.length];
    return f[Math.min(i, f.length - 1)];
  }
  const n = Math.floor((t / 1000) * fps);
  const intro = st.intro ?? [];
  if (n < intro.length) return intro[n];
  const k = n - intro.length;
  if (st.mode === "once") return st.frames[Math.min(k, st.frames.length - 1)];
  return st.frames[k % st.frames.length];
}
