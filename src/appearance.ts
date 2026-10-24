// Turns the Appearance settings into CSS variables. Shared by the main window and
// the dictation pop-up so both always match. Values follow the macOS 26 kit.
export interface Appearance {
  material: "glass" | "frosted" | "solid";
  thickness: number; // 0 Ultra Thin .. 4 Ultra Thick
  tint: string;
  tint_strength: number;
  accent: string;
  roundness: number;
  wallpaper_through: boolean;
  bar_size: "compact" | "regular" | "large";
  show_face: boolean;
}

export const THICKNESS = ["Ultra Thin", "Thin", "Medium", "Thick", "Ultra Thick"];
// the kit's material opacities for #F6F6F6
const ALPHA = [0.36, 0.48, 0.6, 0.72, 0.84];

export const ACCENTS: [string, string][] = [
  ["Blue", "#0d6fff"], ["Purple", "#a550a7"], ["Pink", "#f74f9e"], ["Red", "#ff383c"],
  ["Orange", "#ff8a00"], ["Yellow", "#ffc600"], ["Green", "#34c759"], ["Graphite", "#8c8c8c"],
];
export const TINTS: [string, string][] = [
  ["Clear", "#ffffff"], ["Warm", "#ffd9a8"], ["Rose", "#ffc4d6"], ["Mint", "#b8f0d0"], ["Sky", "#bcdcff"], ["Lilac", "#dcccff"],
];

function rgb(hex: string): [number, number, number] {
  const h = hex.replace("#", "");
  const n = parseInt(h.length === 3 ? h.split("").map((c) => c + c).join("") : h, 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}
const mix = (a: [number, number, number], b: [number, number, number], t: number) => a.map((v, i) => Math.round(v + (b[i] - v) * t)) as [number, number, number];

export function applyAppearance(a: Appearance, root: HTMLElement = document.documentElement) {
  const base = mix([246, 246, 246], rgb(a.tint), Math.max(0, Math.min(1, a.tint_strength)));
  const alpha = a.material === "solid" ? 1 : ALPHA[Math.max(0, Math.min(4, a.thickness))];
  const [ar, ag, ab] = rgb(a.accent);
  const v = (k: string, val: string) => root.style.setProperty(k, val);
  v("--mat", `rgba(${base[0]}, ${base[1]}, ${base[2]}, ${alpha})`);
  v("--mat-strong", `rgba(${base[0]}, ${base[1]}, ${base[2]}, ${Math.min(1, alpha + 0.14)})`);
  v("--mat-blur", a.material === "solid" ? "none" : a.material === "glass" ? "blur(30px) saturate(1.8)" : "blur(15px)");
  // Liquid Glass catches light on its rim; frosted and solid don't
  v("--mat-rim", a.material === "glass"
    ? "inset 1px 1px 0.5px -0.5px rgba(255,255,255,0.9), inset -1px -1px 0.5px -0.5px rgba(255,255,255,0.7), inset 0 0 0 0.5px rgba(255,255,255,0.35)"
    : "inset 0 0 0 0.5px rgba(0,0,0,0.04)");
  v("--mat-shadow", a.material === "solid" ? "0 0 0 0.5px rgba(0,0,0,0.08), 0 4px 16px rgba(0,0,0,0.06)" : "0 8px 40px rgba(0,0,0,0.12)");
  v("--accent", a.accent);
  // the pixel companion wears the accent colour too
  v("--pixel", a.accent);
  v("--mouth", `rgba(${ar}, ${ag}, ${ab}, 0.45)`);
  v("--accent-rgb", `${ar}, ${ag}, ${ab}`);
  v("--r", String(Math.max(0.4, Math.min(1.6, a.roundness))));
  root.dataset.material = a.material;
  root.dataset.bar = a.bar_size;
  root.dataset.face = a.show_face ? "on" : "off";
  root.dataset.wallpaper = a.wallpaper_through ? "on" : "off";
}
