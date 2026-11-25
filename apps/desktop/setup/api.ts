// Calls to the installer's Rust side (`apps/setup/src/app.rs`) and the
// shapes it sends back.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Relation = "fresh" | "older" | "same" | "newer" | "unknown";

export interface Context {
  mode: "install" | "uninstall";
  lang: "en" | "fr";
  version: string;
  installedVersion: string | null;
  relation: Relation;
  dir: string;
}

export type Program = "app" | "terminal";

export type Step =
  | "files"
  | "path"
  | "notifications"
  | "autostart"
  | "restore"
  | "undo"
  | "remove";

export type StepState = "active" | "done" | "failed";

export type Failure =
  | { kind: "diskFull" }
  | { kind: "blocked"; file: string }
  | { kind: "inUse"; file: string }
  | { kind: "other"; message: string };

export interface FailedStep {
  step: Step;
  message: string;
}

export type Outcome =
  | { result: "done" }
  | { result: "partial"; failed: FailedStep[] }
  | { result: "cancelled" }
  | { result: "failed"; failure: Failure; step: Step };

export const api = {
  context: () => invoke<Context>("context"),
  running: () => invoke<Program[]>("running"),
  closeRunning: () => invoke<boolean>("close_running"),
  start: (uninstall: boolean, restore: boolean) =>
    invoke<boolean>("start", { uninstall, restore }),
  retry: (steps: Step[]) => invoke<FailedStep[]>("retry", { steps }),
  cancel: () => invoke<void>("cancel"),
  launchApp: () => invoke<void>("launch_app"),
  openLog: () => invoke<void>("open_log"),
  quit: () => invoke<void>("quit"),
};

export const events = {
  step: (handler: (event: { step: Step; state: StepState }) => void): Promise<UnlistenFn> =>
    listen<{ step: Step; state: StepState }>("setup:step", (e) => handler(e.payload)),
  progress: (handler: (share: number) => void): Promise<UnlistenFn> =>
    listen<number>("setup:progress", (e) => handler(e.payload)),
  committed: (handler: () => void): Promise<UnlistenFn> =>
    listen("setup:committed", () => handler()),
  done: (handler: (outcome: Outcome) => void): Promise<UnlistenFn> =>
    listen<Outcome>("setup:done", (e) => handler(e.payload)),
  closeRequested: (handler: () => void): Promise<UnlistenFn> =>
    listen("setup:close-requested", () => handler()),
};
