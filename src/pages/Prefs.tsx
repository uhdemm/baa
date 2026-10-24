import { useEffect, useState, type KeyboardEvent } from "react";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { api, AutoSend, Mode, Settings } from "../api";
import type { PageProps } from "../App";
import { Keys, Row, Select, Toggle } from "../ui/controls";

const LANGS: [string, string][] = [
  ["", "Detect automatically"], ["en", "English"], ["es", "Spanish"], ["fr", "French"], ["de", "German"],
  ["it", "Italian"], ["pt", "Portuguese"], ["nl", "Dutch"], ["pl", "Polish"], ["ru", "Russian"],
  ["uk", "Ukrainian"], ["tr", "Turkish"], ["ja", "Japanese"], ["ko", "Korean"], ["zh", "Chinese"], ["ar", "Arabic"], ["hi", "Hindi"],
];

const MOD_CODES: Record<string, string> = { ControlLeft: "Ctrl", ControlRight: "Ctrl", AltLeft: "Alt", AltRight: "Alt", ShiftLeft: "Shift", ShiftRight: "Shift", MetaLeft: "Win", MetaRight: "Win" };
const ORDER = ["Ctrl", "Alt", "Shift", "Win"];

// KeyboardEvent.code -> the key names the shortcut parser understands
function keyName(code: string): string | null {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit\d$/.test(code)) return code.slice(5);
  if (/^F\d{1,2}$/.test(code)) return code;
  const map: Record<string, string> = {
    Space: "Space", Backquote: "Backquote", Minus: "Minus", Equal: "Equal", BracketLeft: "BracketLeft", BracketRight: "BracketRight",
    Backslash: "Backslash", Semicolon: "Semicolon", Quote: "Quote", Comma: "Comma", Period: "Period", Slash: "Slash",
    Insert: "Insert", Home: "Home", End: "End", PageUp: "PageUp", PageDown: "PageDown", Pause: "Pause", ScrollLock: "ScrollLock",
  };
  return map[code] ?? null;
}

/** Click, then press a combo. Modifier-only combos (Ctrl+Win) count once you let go. */
function ShortcutInput({ value, onChange, allowModOnly }: { value: string; onChange: (v: string) => void; allowModOnly?: boolean }) {
  const [capturing, setCapturing] = useState(false);
  const [held, setHeld] = useState<string[]>([]);
  const mods = (e: KeyboardEvent) => ORDER.filter((m) => (m === "Ctrl" && e.ctrlKey) || (m === "Alt" && e.altKey) || (m === "Shift" && e.shiftKey) || (m === "Win" && e.metaKey));
  return (
    <button
      type="button"
      className={`shortcut-btn ${capturing ? "capturing" : ""}`}
      onClick={() => { setCapturing(true); setHeld([]); }}
      onBlur={() => setCapturing(false)}
      onKeyDown={(e) => {
        if (!capturing) return;
        e.preventDefault();
        if (e.key === "Escape") return setCapturing(false);
        if (MOD_CODES[e.code]) { setHeld((h) => ORDER.filter((m) => h.includes(m) || m === MOD_CODES[e.code])); return; }
        const k = keyName(e.code);
        const m = mods(e);
        if (!k || (!m.length && !/^F\d/.test(k))) return; // a bare letter would hijack typing
        onChange([...m, k].join("+"));
        setCapturing(false);
      }}
      onKeyUp={(e) => {
        if (!capturing || !allowModOnly || !MOD_CODES[e.code]) return;
        if (held.length >= 2) { onChange(held.join("+")); setCapturing(false); }
      }}
    >
      {capturing ? <Keys combo={held.length ? held.join("+") : "Press keys…"} /> : <Keys combo={value} />}
    </button>
  );
}

export default function Prefs({ settings, save }: PageProps) {
  const [mics, setMics] = useState<string[]>([]);
  const [autostart, setAutostart] = useState(false);
  useEffect(() => {
    api.listMicrophones().then(setMics);
    isEnabled().then(setAutostart).catch(() => {});
  }, []);
  const set = <K extends keyof Settings>(k: K, v: Settings[K]) => save({ ...settings, [k]: v });

  return (
    <>
      <div className="page-head">
        <h1>Settings</h1>
        <p>Customize Baa to your workflow.</p>
      </div>

      <div className="section">
        <h2>Shortcuts</h2>
        <div className="rows" style={{ marginTop: 12 }}>
          <Row title="Dictation" desc="Hold to talk anywhere. Ctrl+Win style combos work too.">
            <ShortcutInput value={settings.shortcut} onChange={(v) => set("shortcut", v)} allowModOnly />
          </Row>
          <Row title="Command mode" desc="Hold over selected text and say what to change.">
            <ShortcutInput value={settings.command_shortcut} onChange={(v) => set("command_shortcut", v)} />
          </Row>
          <Row title="Paste last again" desc="For when the words landed in the wrong box.">
            <ShortcutInput value={settings.repaste_shortcut} onChange={(v) => set("repaste_shortcut", v)} />
          </Row>
          <Row title="How it listens" desc="Hold, tap, or both.">
            <Select width={250} value={settings.mode} onChange={(v) => set("mode", v as Mode)} options={[
              { value: "hybrid", label: "Hold, or tap to start and stop" },
              { value: "hold", label: "Only while I hold it" },
              { value: "toggle", label: "Tap to start, tap to stop" },
            ]} />
          </Row>
        </div>
      </div>

      <div className="section">
        <h2>Voice</h2>
        <div className="rows" style={{ marginTop: 12 }}>
          <Row title="Microphone input" desc="Select your preferred microphone.">
            <Select width={250} value={settings.microphone ?? ""} onChange={(v) => set("microphone", v || null)}
              options={[{ value: "", label: "Windows default" }, ...mics.map((m) => ({ value: m, label: m }))]} />
          </Row>
          <Row title="Language" desc="Leave on automatic unless the model keeps guessing wrong.">
            <Select width={250} value={settings.language ?? ""} onChange={(v) => set("language", v || null)} options={LANGS.map(([value, label]) => ({ value, label }))} />
          </Row>
          <Row title="Cut out silence" desc="Skips the quiet parts so the model works less.">
            <Toggle on={settings.use_vad} onChange={(v) => set("use_vad", v)} />
          </Row>
          <Row title="Remove “um” and “uh”" desc="Filler words never make it into your text.">
            <Toggle on={settings.remove_fillers} onChange={(v) => set("remove_fillers", v)} />
          </Row>
        </div>
      </div>

      <div className="section">
        <h2>Pasting</h2>
        <div className="rows" style={{ marginTop: 12 }}>
          <Row title="After pasting" desc="Press Enter for you, like hitting send.">
            <Select width={250} value={settings.auto_send} onChange={(v) => set("auto_send", v as AutoSend)} options={[
              { value: "off", label: "Just paste" }, { value: "enter", label: "Paste and press Enter" }, { value: "ctrl_enter", label: "Paste and press Ctrl+Enter" },
            ]} />
          </Row>
          <Row title="Add a space after" desc="So the next dictation doesn't run into the last one.">
            <Toggle on={settings.trailing_space} onChange={(v) => set("trailing_space", v)} />
          </Row>
          <Row title="Keep my clipboard" desc="Puts back whatever you had copied.">
            <Toggle on={settings.restore_clipboard} onChange={(v) => set("restore_clipboard", v)} />
          </Row>
          <Row title="Learn my corrections" desc="Offers to remember words you fix after pasting.">
            <Toggle on={settings.learn_words} onChange={(v) => set("learn_words", v)} />
          </Row>
        </div>
      </div>

      <div className="section">
        <h2>The bar</h2>
        <div className="rows" style={{ marginTop: 12 }}>
          <Row title="Show the bar" desc="The little bar that appears while you talk.">
            <Toggle on={settings.show_overlay} onChange={(v) => set("show_overlay", v)} />
          </Row>
          <Row title="Live transcript" desc="Show your words in the bar as you speak.">
            <Toggle on={settings.live_preview} onChange={(v) => set("live_preview", v)} />
          </Row>
          <Row title="Position" desc="Where on screen the bar appears.">
            <Select width={200} value={settings.overlay_position} onChange={(v) => set("overlay_position", v)} options={[
              { value: "bottom", label: "Bottom center" }, { value: "top", label: "Top center" }, { value: "bottom_left", label: "Bottom left" },
              { value: "bottom_right", label: "Bottom right" }, { value: "top_left", label: "Top left" }, { value: "top_right", label: "Top right" },
            ]} />
          </Row>
          <Row title="Sounds" desc="Soft blips when Baa starts and stops listening.">
            <Toggle on={settings.sounds} onChange={(v) => set("sounds", v)} />
          </Row>
        </div>
      </div>

      <div className="section">
        <h2>System</h2>
        <div className="rows" style={{ marginTop: 12 }}>
          <Row title="Keep the model loaded" desc="Longer is snappier; shorter frees memory.">
            <Select width={250} value={settings.unload_after_secs} onChange={(v) => set("unload_after_secs", v)} options={[
              { value: 60, label: "1 minute after use" }, { value: 300, label: "5 minutes after use" }, { value: 1800, label: "30 minutes after use" }, { value: 0, label: "Always" },
            ]} />
          </Row>
          <Row title="Launch at login" desc="Start Baa automatically when you log in.">
            <Toggle on={autostart} onChange={async (v) => { await (v ? enable() : disable()); setAutostart(await isEnabled()); }} />
          </Row>
        </div>
      </div>
    </>
  );
}
