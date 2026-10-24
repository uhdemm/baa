import { invoke } from "@tauri-apps/api/core";

export type Mode = "hybrid" | "hold" | "toggle";
export type AutoSend = "off" | "enter" | "ctrl_enter";

export interface Replacement { from: string; to: string }
export interface Snippet { trigger: string; text: string }
export interface AppProfile { exe: string; auto_send: AutoSend | null; style: string | null }

export interface Settings {
  shortcut: string;
  mode: Mode;
  model: string | null;
  imported_models: string[];
  language: string | null;
  microphone: string | null;
  auto_send: AutoSend;
  restore_clipboard: boolean;
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
}

export interface EngineStatus { loaded: string | null; loading: boolean; backend: string | null }

export interface DownloadProgress {
  id: string;
  done: number;
  total: number;
  state: "downloading" | "verifying" | "done" | "error" | "cancelled";
  error: string | null;
}

export const api = {
  getSettings: () => invoke<Settings>("get_settings"),
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
};

export const fmtSize = (b: number) =>
  b >= 1e9 ? `${(b / 1e9).toFixed(1)} GB` : `${Math.round(b / 1e6)} MB`;
