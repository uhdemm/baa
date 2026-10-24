import { invoke } from "@tauri-apps/api/core";
import type { Appearance } from "./appearance";

export type Mode = "hybrid" | "hold" | "toggle";
export type AutoSend = "off" | "enter" | "ctrl_enter";

export interface Replacement { from: string; to: string }
export interface Snippet { trigger: string; text: string }
export interface AppProfile { exe: string; auto_send: AutoSend | null; style: string | null }

export interface AiSettings {
  cleanup: boolean;
  provider: "anthropic" | "openai" | "groq" | "gemini" | "cerebras" | "openrouter" | "freellmapi" | "ollama" | "custom";
  base_url: string;
  model: string;
  prompt: string;
  timeout_secs: number;
}

export type Style = "formal" | "casual" | "very_casual" | "excited";
export interface Styles { email: Style; work_chat: Style; personal_chat: Style; other: Style }

export interface Settings {
  shortcut: string;
  mode: Mode;
  command_shortcut: string;
  repaste_shortcut: string;
  live_preview: boolean;
  mouse_button: "" | "mouse4" | "mouse5";
  translate_to_english: boolean;
  whisper_mode: boolean;
  mute_while_dictating: boolean;
  smart_continue: boolean;
  styles: Styles;
  ai: AiSettings;
  appearance: Appearance;
  learn_words: boolean;
  word_suggestions: string[];
  model: string | null;
  imported_models: string[];
  language: string | null;
  microphone: string | null;
  auto_send: AutoSend;
  restore_clipboard: boolean;
  rich_paste: boolean;
  trailing_space: boolean;
  remove_fillers: boolean;
  use_vad: boolean;
  custom_words: string[];
  replacements: Replacement[];
  snippets: Snippet[];
  app_profiles: AppProfile[];
  unload_after_secs: number;
  overlay_position: string;
  show_overlay: boolean;
  sounds: boolean;
  overlay_style: "bar" | "buddy";
  buddy_place: "taskbar" | "popup" | "free";
  buddy_size: "small" | "medium" | "large";
  buddy_x: number;
  buddy_y: number;
  buddy_sleeps: boolean;
  character: string;
  onboarded: boolean;
}

export interface ModelRow {
  id: string;
  name: string;
  description: string;
  tier: string;
  size: number;
  installed: boolean;
  downloading: boolean;
  imported: boolean;
  path: string | null;
}

export interface Entry {
  id: number;
  ts: number;
  text: string;
  raw: string;
  app: string | null;
  words: number;
  audio_ms: number;
  model: string | null;
}

export interface Stats {
  total_words: number;
  total_dictations: number;
  words_today: number;
  words_week: number;
  streak_days: number;
  minutes_saved: number;
  wpm: number;
  daily: [string, number][];
  top_apps: [string, number][];
}

export interface Note { id: number; created: number; updated: number; title: string; body: string; kind: "note" | "meeting" }

export type Template = "general" | "one_on_one" | "sales" | "standup" | "interview";
export interface MeetingStatus {
  running: boolean; finishing: boolean; stage: string; seconds: number; transcript: string;
  hears_call: boolean; speakers: boolean; attendees: string; my_notes: string; template: Template;
}

export interface EngineStatus { loaded: string | null; loading: boolean; backend: string | null }

export interface DownloadProgress {
  id: string;
  done: number;
  total: number;
  state: "downloading" | "verifying" | "done" | "error" | "cancelled";
  error: string | null;
}

export interface AiStatus { ready: boolean; keys: string[] }

export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
  startupWarning: () => invoke<string | null>("startup_warning"),
  setSettings: (settings: Settings) => invoke<Settings>("set_settings", { settings }),
  listModels: () => invoke<ModelRow[]>("list_models"),
  downloadModel: (id: string) => invoke<void>("download_model", { id }),
  cancelDownload: (id: string) => invoke<void>("cancel_download", { id }),
  deleteModel: (id: string) => invoke<void>("delete_model", { id }),
  importModel: (path: string) => invoke<string>("import_model", { path }),
  engineStatus: () => invoke<EngineStatus>("engine_status"),
  listMicrophones: () => invoke<string[]>("list_microphones"),
  historyList: (limit: number, offset: number, query?: string) =>
    invoke<Entry[]>("history_list", { limit, offset, query: query || null }),
  historyDelete: (id: number) => invoke<void>("history_delete", { id }),
  historyClear: () => invoke<void>("history_clear"),
  stats: () => invoke<Stats>("stats"),
  repasteLast: () => invoke<void>("repaste_last"),
  previewText: (raw: string) => invoke<string>("preview_text", { raw }),
  micTestStart: () => invoke<void>("mic_test_start"),
  micTestStop: () => invoke<void>("mic_test_stop"),
  aiStatus: () => invoke<AiStatus>("ai_status"),
  setApiKey: (provider: string, key: string) => invoke<void>("set_api_key", { provider, key }),
  testAi: (sample: string, context?: string) => invoke<string>("test_ai", { sample, context: context ?? null }),
  defaultCleanupPrompt: () => invoke<string>("default_cleanup_prompt"),
  listAiModels: () => invoke<string[]>("list_ai_models"),
  historyExport: (path: string) => invoke<number>("history_export", { path }),
  notesList: (query?: string) => invoke<Note[]>("notes_list", { query: query || null }),
  noteSave: (id: number | null, title: string, body: string) => invoke<number>("note_save", { id, title, body }),
  noteDelete: (id: number) => invoke<void>("note_delete", { id }),
  meetingStart: () => invoke<void>("meeting_start"),
  meetingStop: () => invoke<void>("meeting_stop"),
  meetingStatus: () => invoke<MeetingStatus>("meeting_status"),
  meetingUpdate: (attendees: string, myNotes: string, template: Template) =>
    invoke<void>("meeting_update", { attendees, myNotes, template }),
  noteAsk: (title: string, body: string, question: string) => invoke<string>("note_ask", { title, body, question }),
};

export const fmtSize = (b: number) =>
  b >= 1e9 ? `${(b / 1e9).toFixed(1)} GB` : `${Math.round(b / 1e6)} MB`;
