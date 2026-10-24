// Dev-only: when the UI is opened in a plain browser (no Tauri backend), answer backend
// calls with sample data so every screen can be previewed. Never bundled into the app.
const settings = {
  shortcut: "Ctrl+Space", mode: "hybrid", command_shortcut: "Alt+Shift+Space", repaste_shortcut: "Alt+Shift+V",
  live_preview: true, learn_words: true,
  appearance: { material: "glass", thickness: 2, tint: "#ffffff", tint_strength: 0, accent: "#0d6fff", roundness: 1, wallpaper_through: true, bar_size: "regular", show_face: true }, word_suggestions: ["Remotion"],
  ai: { cleanup: false, provider: "groq", base_url: "", model: "llama-3.3-70b-versatile", prompt: "You clean up dictated text.", timeout_secs: 6 },
  model: "parakeet-tdt-0.6b-v3", imported_models: [], language: null, microphone: null, auto_send: "off",
  restore_clipboard: true, trailing_space: true, remove_fillers: true, use_vad: true,
  custom_words: ["Big Square", "Baa"], replacements: [{ from: "big square", to: "Big Square" }],
  snippets: [{ trigger: "my email", text: "adam@example.com" }], app_profiles: [{ exe: "discord.exe", auto_send: "enter", style: "lowercase" }],
  unload_after_secs: 300, overlay_position: "bottom", show_overlay: true, sounds: true, character: "sheep",
  onboarded: !location.search.includes("setup"),
};
const now = Math.floor(Date.now() / 1000);
const data: Record<string, unknown> = {
  get_settings: settings, set_settings: settings, startup_warning: null,
  stats: { total_words: 4182, total_dictations: 211, words_today: 640, words_week: 2390, streak_days: 6, minutes_saved: 87 },
  history_list: [
    { id: 3, ts: now - 40, text: "Just taking some notes on this idea. I think the new design is feeling really good.", raw: "", app: "notion.exe", words: 16, audio_ms: 5200, model: null },
    { id: 2, ts: now - 3600, text: "Can you send the Remotion render to Big Square before Friday?", raw: "", app: "discord.exe", words: 11, audio_ms: 3800, model: null },
  ],
  list_models: [
    { id: "parakeet-tdt-0.6b-v3", name: "Parakeet v3", description: "Fast and accurate. 25 European languages.", tier: "recommended", size: 739508576, installed: true, downloading: false, imported: false, path: "x" },
    { id: "parakeet-unified-en-0.6b", name: "Parakeet English", description: "Best English accuracy for its size.", tier: "recommended", size: 731357568, installed: false, downloading: false, imported: false, path: null },
    { id: "whisper-large-v3-turbo", name: "Whisper Turbo", description: "99 languages. Needs a GPU to feel quick.", tier: "great", size: 886000000, installed: false, downloading: false, imported: false, path: null },
  ],
  engine_status: { loaded: "x", loading: false, backend: "Vulkan0" },
  list_microphones: ["HyperX SoloCast", "Realtek Microphone"],
  ai_status: { ready: true, keys: ["groq"] },
  list_ai_models: ["llama-3.3-70b-versatile", "llama-3.1-8b-instant", "openai/gpt-oss-120b", "openai/gpt-oss-20b", "qwen/qwen3-32b", "moonshotai/kimi-k2-instruct"],
  preview_text: "So send it to adam@example.com please ",
  "plugin:autostart|is_enabled": false,
};

export function installDevMock() {
  const w = window as unknown as Record<string, unknown>;
  if (w.__TAURI_INTERNALS__) return;
  let cb = 0;
  w.__TAURI_INTERNALS__ = {
    invoke: async (cmd: string) => (cmd in data ? structuredClone(data[cmd]) : cmd.startsWith("plugin:event") ? 0 : null),
    transformCallback: () => ++cb,
    unregisterCallback: () => {},
    convertFileSrc: (p: string) => p,
    metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main", windowLabel: "main" } },
  };
  w.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
}
