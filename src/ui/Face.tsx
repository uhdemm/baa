// The pixel companion, drawn from the same pixel maps as the Figma "Face" component.
export type Mood = "idle" | "listening" | "thinking" | "happy" | "typing" | "success";

const FACE = [
  "....XXXXXXXX....",
  "...X........X...",
  "..X..........X..",
  ".X............X.",
  "X..............X",
  "X..............X",
  "X..............X",
  "X..............X",
  "X..............X",
  "X..............X",
  ".X............X.",
  "..X..........X..",
  "...X.XXXXXX.X...",
  "...XX......XX...",
];
type Px = [number, number];
const bar = (r: number): Px[] => [[5, r], [5, r + 1], [5, r + 2], [10, r], [10, r + 1], [10, r + 2]];
const arc = (r: number): Px[] => [[5, r], [4, r + 1], [6, r + 1], [10, r], [9, r + 1], [11, r + 1]];

// [face-space outline extras, face-space mouth pixels, canvas-space extras]
const MOODS: Record<Mood, [Px[], Px[], Px[]]> = {
  idle: [bar(5), [], []],
  listening: [bar(6), [[6, 10], [7, 10], [8, 10], [9, 10]], [[0, 4], [2, 2], [4, 1]]],
  thinking: [[...bar(5), [7, 10], [8, 10]], [[7, 9], [8, 9]], []],
  happy: [[...arc(5), [5, 8], [10, 8], [6, 9], [7, 9], [8, 9], [9, 9]], [], []],
  typing: [[...arc(6), [7, 13], [8, 13], [7, 14], [8, 14]], [], [[9, 1], [10, 2], [11, 1], [12, 0]]],
  success: [arc(6), [[5, 9], [10, 9], [6, 10], [7, 10], [8, 10], [9, 10]], [[14, 0], [16, 0], [15, 1], [14, 2], [16, 2], [18, 2], [17, 3], [19, 3], [18, 4]]],
};

const OX = 2, OY = 4;

/** `size` is the width of the face body (the 16-pixel head); the canvas adds a 2-pixel margin each side. */
export default function Face({ mood = "idle", size = 32, extras = true, className }: { mood?: Mood; size?: number; extras?: boolean; className?: string }) {
  const [dark, mouth, xdark] = MOODS[mood];
  const cells = new Map<string, "d" | "m">();
  FACE.forEach((row, y) => [...row].forEach((ch, x) => ch === "X" && cells.set(`${x + OX},${y + OY}`, "d")));
  dark.forEach(([x, y]) => cells.set(`${x + OX},${y + OY}`, "d"));
  mouth.forEach(([x, y]) => cells.set(`${x + OX},${y + OY}`, "m"));
  if (extras) xdark.forEach(([x, y]) => cells.set(`${x},${y}`, "d"));
  const u = size / 16;
  return (
    <svg className={className} width={20 * u} height={19 * u} viewBox="0 0 20 19" shapeRendering="crispEdges" aria-hidden="true">
      {[...cells].map(([k, kind]) => {
        const [x, y] = k.split(",").map(Number);
        return <rect key={k} x={x} y={y} width={1.02} height={1.02} fill={kind === "d" ? "var(--pixel)" : "var(--mouth)"} />;
      })}
    </svg>
  );
}
