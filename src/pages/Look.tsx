import { ACCENTS, Appearance, THICKNESS, TINTS } from "../appearance";
import type { PageProps } from "../App";
import Face from "../ui/Face";
import Wave from "../ui/Wave";
import { Row, Toggle } from "../ui/controls";

function Seg<T extends string>({ value, options, onChange }: { value: T; options: [T, string][]; onChange: (v: T) => void }) {
  return (
    <div className="seg" role="radiogroup">
      {options.map(([v, l]) => <button key={v} role="radio" aria-checked={v === value} className={v === value ? "on" : ""} onClick={() => onChange(v)}>{l}</button>)}
    </div>
  );
}

function Swatches({ value, colors, onChange }: { value: string; colors: [string, string][]; onChange: (v: string) => void }) {
  const custom = !colors.some(([, c]) => c.toLowerCase() === value.toLowerCase());
  return (
    <div className="swatches">
      {colors.map(([name, c]) => (
        <button key={c} title={name} aria-label={name} className={`swatch ${c.toLowerCase() === value.toLowerCase() ? "on" : ""}`} style={{ background: c }} onClick={() => onChange(c)} />
      ))}
      <span title="Custom colour" className={`swatch ${custom ? "on" : ""}`} style={{ background: custom ? value : "conic-gradient(red, yellow, lime, cyan, blue, magenta, red)" }}>
        <input type="color" value={value} onChange={(e) => onChange(e.target.value)} aria-label="Custom colour" />
      </span>
    </div>
  );
}

function Slider({ value, min, max, step, onChange, ticks }: { value: number; min: number; max: number; step: number; onChange: (v: number) => void; ticks?: string[] }) {
  const p = ((value - min) / (max - min)) * 100;
  return (
    <div>
      <input className="slider" type="range" min={min} max={max} step={step} value={value} style={{ ["--p" as string]: `${p}%` }} onChange={(e) => onChange(Number(e.target.value))} />
      {ticks && <div className="ticks">{ticks.map((t, i) => <span key={i}>{t}</span>)}</div>}
    </div>
  );
}

export default function Look({ settings, save }: PageProps) {
  const a = settings.appearance;
  const set = (patch: Partial<Appearance>) => save({ ...settings, appearance: { ...a, ...patch } });

  return (
    <>
      <div className="preview-stage">
        <div className="sample glass">
          <b>Sidebar</b>
          <span>{a.material === "solid" ? "Solid" : `${a.material === "glass" ? "Liquid Glass" : "Frosted"}, ${THICKNESS[a.thickness]}`}</span>
          <div className="toggle on" style={{ marginTop: 6 }}><i /></div>
        </div>
        <div className="ov-sample">
          <div className="dbar">
            <div className="face-wrap"><Face mood="listening" size={29} extras={false} /></div>
            <Wave style="listening" level={0.5} />
            <div className="divider" />
            <span className="label">Listening...</span>
            <i className="caret" />
          </div>
        </div>
      </div>

      <div className="section">
        <h2>Material</h2>
        <div className="rows">
          <Row title="Material" desc="Liquid Glass catches light on its edges; Frosted is the classic blur; Solid is plain.">
            <Seg value={a.material} options={[["glass", "Liquid Glass"], ["frosted", "Frosted"], ["solid", "Solid"]]} onChange={(material) => set({ material })} />
          </Row>
          <Row title="Thickness" desc="How much of what's behind shows through.">
            <Slider value={a.thickness} min={0} max={4} step={1} onChange={(thickness) => set({ thickness })} ticks={["Thin", "", "Medium", "", "Thick"]} />
          </Row>
          <Row title="Tint" desc="Colour the glass.">
            <Swatches value={a.tint} colors={TINTS} onChange={(tint) => set({ tint, tint_strength: tint.toLowerCase() === "#ffffff" ? 0 : Math.max(a.tint_strength, 0.35) })} />
          </Row>
          <Row title="Tint strength">
            <Slider value={a.tint_strength} min={0} max={1} step={0.05} onChange={(tint_strength) => set({ tint_strength })} />
          </Row>
          <Row title="Wallpaper through the sidebar" desc="Your desktop glows softly behind the glass (Windows 11).">
            <Toggle on={a.wallpaper_through} onChange={(wallpaper_through) => set({ wallpaper_through })} />
          </Row>
        </div>
      </div>

      <div className="section">
        <h2>Colour and shape</h2>
        <div className="rows">
          <Row title="Accent colour" desc="Selections, switches, buttons and the waveform.">
            <Swatches value={a.accent} colors={ACCENTS} onChange={(accent) => set({ accent })} />
          </Row>
          <Row title="Corner roundness">
            <Slider value={a.roundness} min={0.5} max={1.5} step={0.05} onChange={(roundness) => set({ roundness })} ticks={["Sharp", "", "Round"]} />
          </Row>
        </div>
      </div>

      <div className="section">
        <h2>Dictation pop-up</h2>
        <div className="rows">
          <Row title="Size">
            <Seg value={a.bar_size} options={[["compact", "Compact"], ["regular", "Regular"], ["large", "Large"]]} onChange={(bar_size) => set({ bar_size })} />
          </Row>
          <Row title="Show the face" desc="The little pixel companion that reacts while you talk.">
            <Toggle on={a.show_face} onChange={(show_face) => set({ show_face })} />
          </Row>
        </div>
      </div>

      <div style={{ display: "flex", justifyContent: "flex-end" }}>
        <button className="btn" onClick={() => save({ ...settings, appearance: { material: "glass", thickness: 2, tint: "#ffffff", tint_strength: 0, accent: "#0d6fff", roundness: 1, wallpaper_through: true, bar_size: "regular", show_face: true } })}>
          Restore defaults
        </button>
      </div>
    </>
  );
}
