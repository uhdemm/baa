import { useEffect, useState, type ReactNode } from "react";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { api, AutoSend, Mode, Settings } from "../api";
import type { PageProps } from "../App";

const LANGS: [string, string][] = [
  ["", "Detect automatically"], ["en", "English"], ["es", "Spanish"], ["fr", "French"], ["de", "German"],
  ["it", "Italian"], ["pt", "Portuguese"], ["nl", "Dutch"], ["pl", "Polish"], ["ru", "Russian"],
  ["uk", "Ukrainian"], ["tr", "Turkish"], ["ja", "Japanese"], ["ko", "Korean"], ["zh", "Chinese"], ["ar", "Arabic"], ["hi", "Hindi"],
];

// KeyboardEvent.code -> the key names the global shortcut parser understands
function keyName(code: string): string | null {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit\d$/.test(code)) return code.slice(5);
  if (/^F\d{1,2}$/.test(code)) return code;
  const map: Record<string, string> = {
    Space: "Space", Backquote: "Backquote", Minus: "Minus", Equal: "Equal", BracketLeft: "BracketLeft",
    BracketRight: "BracketRight", Backslash: "Backslash", Semicolon: "Semicolon", Quote: "Quote", Comma: "Comma",
    Period: "Period", Slash: "Slash", Insert: "Insert", Home: "Home", End: "End", PageUp: "PageUp", PageDown: "PageDown",
    Pause: "Pause", ScrollLock: "ScrollLock",
  };
  return map[code] ?? null;
}

function ShortcutInput({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const [capturing, setCapturing] = useState(false);
  return (
    <button
      className={`shortcut ${capturing ? "capturing" : ""}`}
      onClick={() => setCapturing(true)}
      onBlur={() => setCapturing(false)}
      onKeyDown={(e) => {
        if (!capturing) return;
        e.preventDefault();
        if (e.key === "Escape") return setCapturing(false);
        const k = keyName(e.code);
        if (!k) return; // still holding modifiers only
        const mods = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Super"].filter(Boolean);
        if (!mods.length && !/^F\d/.test(k)) return; // a bare letter would hijack typing
        onChange([...mods, k].join("+"));
        setCapturing(false);
      }}
    >
      {capturing ? "Press your new shortcut…" : value}
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
      <h1>Settings</h1>
      <div className="form">
        <label>Shortcut</label>
        <ShortcutInput value={settings.shortcut} onChange={(v) => set("shortcut", v)} />

        <label>Command mode</label>
        <ShortcutInput value={settings.command_shortcut} onChange={(v) => set("command_shortcut", v)} />

        <label>Paste last again</label>
        <ShortcutInput value={settings.repaste_shortcut} onChange={(v) => set("repaste_shortcut", v)} />

        <label>How it listens</label>
        <select value={settings.mode} onChange={(e) => set("mode", e.target.value as Mode)}>
          <option value="hybrid">Hold to talk, or tap once to start and again to stop</option>
          <option value="hold">Only while I hold the shortcut</option>
          <option value="toggle">Tap to start, tap to stop</option>
        </select>

        <label>Microphone</label>
        <select value={settings.microphone ?? ""} onChange={(e) => set("microphone", e.target.value || null)}>
          <option value="">Windows default</option>
          {mics.map((m) => <option key={m} value={m}>{m}</option>)}
        </select>

        <label>Language</label>
        <select value={settings.language ?? ""} onChange={(e) => set("language", e.target.value || null)}>
          {LANGS.map(([v, l]) => <option key={v} value={v}>{l}</option>)}
        </select>

        <label>After pasting</label>
        <select value={settings.auto_send} onChange={(e) => set("auto_send", e.target.value as AutoSend)}>
          <option value="off">Just paste</option>
          <option value="enter">Paste and press Enter (send)</option>
          <option value="ctrl_enter">Paste and press Ctrl+Enter</option>
        </select>

        <label>Keep model loaded</label>
        <select value={settings.unload_after_secs} onChange={(e) => set("unload_after_secs", Number(e.target.value))}>
          <option value={60}>1 minute after use (lightest)</option>
          <option value={300}>5 minutes after use</option>
          <option value={1800}>30 minutes after use</option>
          <option value={0}>Always (fastest, uses the most memory)</option>
        </select>

        <label>Bar position</label>
        <select value={settings.overlay_position} onChange={(e) => set("overlay_position", e.target.value)}>
          <option value="bottom">Bottom center</option>
          <option value="top">Top center</option>
          <option value="bottom_left">Bottom left</option>
          <option value="bottom_right">Bottom right</option>
          <option value="top_left">Top left</option>
          <option value="top_right">Top right</option>
        </select>

        <label>Options</label>
        <div className="checks">
          <Check v={settings.show_overlay} on={(v) => set("show_overlay", v)}>Show the bar while I talk</Check>
          <Check v={settings.live_preview} on={(v) => set("live_preview", v)}>Show my words in the bar as I speak</Check>
          <Check v={settings.learn_words} on={(v) => set("learn_words", v)}>Learn words I correct after pasting</Check>
          <Check v={settings.remove_fillers} on={(v) => set("remove_fillers", v)}>Remove "um" and "uh"</Check>
          <Check v={settings.trailing_space} on={(v) => set("trailing_space", v)}>Add a space after each dictation</Check>
          <Check v={settings.restore_clipboard} on={(v) => set("restore_clipboard", v)}>Put my clipboard back after pasting</Check>
          <Check v={settings.use_vad} on={(v) => set("use_vad", v)}>Cut out silence before transcribing</Check>
          <Check v={settings.sounds} on={(v) => set("sounds", v)}>Sounds</Check>
          <Check
            v={autostart}
            on={async (v) => {
              await (v ? enable() : disable());
              setAutostart(await isEnabled());
            }}
          >
            Start Baa when Windows starts
          </Check>
        </div>
      </div>
    </>
  );
}

function Check({ v, on, children }: { v: boolean; on: (v: boolean) => void; children: ReactNode }) {
  return (
    <label className="check">
      <input type="checkbox" checked={v} onChange={(e) => on(e.target.checked)} />
      {children}
    </label>
  );
}
