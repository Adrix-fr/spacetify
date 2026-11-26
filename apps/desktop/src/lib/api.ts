// Calls to the Rust side (`src-tauri/src/commands.rs`), with the shapes it
// sends back. Errors arrive as `Failure`.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface Failure {
  code: string | null;
  message: string;
}

export function isFailure(value: unknown): value is Failure {
  return typeof value === "object" && value !== null && "message" in value;
}

export function failureMessage(value: unknown): string {
  if (isFailure(value)) return value.message;
  return String(value);
}

export interface Swatch {
  label: string;
  color: string;
}

export interface Palette {
  frame: string;
  background: string;
  highlight: string;
  elevated: string;
  text: string;
  accent: string;
  swatches: Swatch[];
}

export interface SpotifyView {
  found: boolean;
  error: string | null;
  path: string | null;
  version: string | null;
  kind: string | null;
  readOnly: boolean;
  state: "original" | "patched" | "spicetify" | "missing" | "unknown" | null;
  stateLabel: string | null;
  running: boolean;
  spicetifyLeftovers: string | null;
  updateReplaced: boolean;
  storeAlsoInstalled: boolean;
  backups: number;
}

export interface ServiceView {
  running: boolean;
  autostart: "on" | "off" | "disabled" | "error";
  autostartDetail: string | null;
  phase: string | null;
  started: string | null;
  lastRepair: string | null;
  installed: boolean;
}

export interface Overview {
  spotify: SpotifyView;
  service: ServiceView;
  theme: { id: string; name: string; palette: Palette } | null;
  extensions: string[];
  modsError: string | null;
}

export interface Done {
  message: string;
  detail: string | null;
}

export interface SettingView {
  name: string;
  label: string;
  kind: "boolean" | "number" | "string" | "choice";
  value: unknown;
  default: unknown;
  choices: string[];
  custom: boolean;
}

export interface ModView {
  id: string;
  name: string;
  version: string | null;
  author: string | null;
  description: string | null;
  active: boolean;
  error: string | null;
  palette: Palette | null;
  settings: SettingView[];
}

export interface ExampleView {
  kind: "theme" | "extension";
  id: string;
  name: string;
  description: string | null;
  installed: boolean;
}

export interface ModsView {
  themes: ModView[];
  extensions: ModView[];
  examples: ExampleView[];
  folder: string;
  original: Palette;
}

export type Verdict = "nothing" | "compatible" | "warnings" | "broken" | "invalid";

export interface DoctorView {
  spotifyVersion: string | null;
  verdict: Verdict;
  summary: string;
  mods: {
    kind: string;
    title: string;
    verdict: Verdict;
    headline: string;
    error: string | null;
    problems: { level: "missing" | "fragile" | "unknown"; text: string }[];
  }[];
}

export interface EventView {
  at: string;
  when: string;
  text: string;
  tone: "good" | "bad" | "warn" | "info";
}

export interface RepairView {
  service: ServiceView;
  events: EventView[];
  log: string;
}

export interface SettingsView {
  language: "system" | "en" | "fr";
  lang: "en" | "fr";
  version: string;
  configDir: string;
  dataDir: string;
}

export type InGallery = "no" | "same" | "older" | "newer";

export interface Candidate {
  folder: string;
  id: string;
  name: string;
  kind: "theme" | "extension";
  version: string;
  palette: Palette | null;
  inGallery: InGallery;
}

export type CheckLevel = "ok" | "look" | "block";

export interface ReviewView {
  candidate: Candidate | null;
  inGallery: InGallery;
  galleryVersion: string | null;
  checks: {
    title: string;
    level: CheckLevel;
    clean: string | null;
    findings: { what: string; place: string | null; excerpt: string | null; level: CheckLevel }[];
  }[];
  summary: string;
  blocking: number;
  toLookAt: number;
  size: number;
}

export const api = {
  overview: () => invoke<Overview>("overview"),
  apply: (closeSpotify: boolean) => invoke<Done>("apply", { closeSpotify }),
  restore: (closeSpotify: boolean) => invoke<Done>("restore", { closeSpotify }),
  launchSpotify: () => invoke<void>("launch_spotify"),
  listMods: () => invoke<ModsView>("list_mods"),
  installMod: (path: string) => invoke<Done>("install_mod", { path }),
  installExample: (id: string) => invoke<Done>("install_example", { id }),
  uninstallMod: (id: string) => invoke<Done>("uninstall_mod", { id }),
  setTheme: (id: string | null) => invoke<void>("set_theme", { id }),
  setExtension: (id: string, on: boolean) => invoke<void>("set_extension", { id, on }),
  setSetting: (id: string, name: string, value: unknown | null) =>
    invoke<void>("set_setting", { id, name, value }),
  doctor: () => invoke<DoctorView>("doctor"),
  repairInfo: (count?: number) => invoke<RepairView>("repair_info", { count }),
  setServiceRunning: (on: boolean) => invoke<Done>("set_service_running", { on }),
  setAutostart: (on: boolean) => invoke<Done>("set_autostart", { on }),
  serviceCheck: () => invoke<void>("service_check"),
  getSettings: () => invoke<SettingsView>("get_settings"),
  setLanguage: (language: string) => invoke<void>("set_language", { language }),
  runCli: (line: string) => invoke<void>("run_cli", { line }),
  stopCli: () => invoke<void>("stop_cli"),
  takeStartPage: () => invoke<string | null>("take_start_page"),
  submitCandidates: () => invoke<Candidate[]>("submit_candidates"),
  submitReview: (folder: string) => invoke<ReviewView>("submit_review", { folder }),
  submitPrepare: (folder: string) =>
    invoke<{ zip: string; formUrl: string }>("submit_prepare", { folder }),
};

export const events = {
  progress: (handler: (text: string) => void): Promise<UnlistenFn> =>
    listen<string>("progress", (e) => handler(e.payload)),
  trayAction: (handler: (action: "apply" | "restore") => void): Promise<UnlistenFn> =>
    listen<"apply" | "restore">("tray-action", (e) => handler(e.payload)),
  terminalLine: (handler: (line: { stream: "out" | "err"; text: string }) => void) =>
    listen<{ stream: "out" | "err"; text: string }>("terminal-line", (e) => handler(e.payload)),
  openPage: (handler: (page: string) => void): Promise<UnlistenFn> =>
    listen<string>("open-page", (e) => handler(e.payload)),
  terminalExit: (handler: (exit: { code: number | null }) => void) =>
    listen<{ code: number | null }>("terminal-exit", (e) => handler(e.payload)),
};
