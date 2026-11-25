// Shared state of the window: language, the latest overview, the long
// operation in progress, messages and the confirmation dialog.
import { api, failureMessage, isFailure, type Overview } from "./api";

export const app = $state({
  lang: "en" as "en" | "fr",
  overview: null as Overview | null,
  /** Label of the long operation in progress, if any. */
  busy: null as string | null,
  page: "home" as Page,
});

export type Page = "home" | "themes" | "extensions" | "compat" | "repair" | "terminal" | "settings";

/** English or French, like `tr!` on the Rust side. */
export function tr(en: string, fr: string): string {
  return app.lang === "fr" ? fr : en;
}

// ---------------------------------------------------------------- messages

export interface Toast {
  id: number;
  tone: "good" | "bad" | "info";
  text: string;
  detail?: string | null;
  action?: { label: string; run: () => void };
}

export const toasts = $state<Toast[]>([]);
let nextToast = 1;

export function toast(tone: Toast["tone"], text: string, extra: Partial<Toast> = {}) {
  const id = nextToast++;
  toasts.push({ id, tone, text: shortLine(text), ...extra, detail: extra.detail && shortLine(extra.detail) });
  setTimeout(() => dismiss(id), tone === "bad" ? 12000 : 6000);
}

/** A one-sentence message reads as a label: no closing period. */
function shortLine(text: string): string {
  const trimmed = text.trim();
  const sentences = trimmed.split(/[.!?](\s|$)/).filter((s) => s && s.trim()).length;
  return sentences <= 1 ? trimmed.replace(/\.$/, "") : trimmed;
}

export function dismiss(id: number) {
  const index = toasts.findIndex((t) => t.id === id);
  if (index >= 0) toasts.splice(index, 1);
}

export function showError(error: unknown) {
  toast("bad", failureMessage(error));
}

// ------------------------------------------------------------ confirmation

export interface Question {
  title: string;
  body: string;
  confirm: string;
  cancel?: string;
  /** Only an OK button: the dialog explains, it does not ask. */
  info?: boolean;
  resolve: (ok: boolean) => void;
}

export const dialog = $state({ current: null as Question | null });

export function ask(question: Omit<Question, "resolve">): Promise<boolean> {
  return new Promise((resolve) => {
    dialog.current = { ...question, resolve };
  });
}

export function answer(ok: boolean) {
  const current = dialog.current;
  dialog.current = null;
  current?.resolve(ok);
}

// --------------------------------------------------------------- overview

export async function refreshOverview() {
  try {
    app.overview = await api.overview();
  } catch (error) {
    showError(error);
  }
}

/** Runs a long operation with the progress overlay. */
export async function withBusy<T>(label: string, job: () => Promise<T>): Promise<T> {
  app.busy = label;
  try {
    return await job();
  } finally {
    app.busy = null;
  }
}

/**
 * Apply or restore, as the Home buttons and the tray menu do. Asks before
 * closing Spotify, and explains when Spicetify is in the way.
 */
export async function runAction(kind: "apply" | "restore") {
  if (app.busy) return;
  const label =
    kind === "apply"
      ? tr("Applying the mods…", "Application des mods…")
      : tr("Putting the original back…", "Remise de l'interface d'origine…");
  const call = (close: boolean) => (kind === "apply" ? api.apply(close) : api.restore(close));
  try {
    let done;
    try {
      done = await withBusy(label, () => call(false));
    } catch (error) {
      if (!isFailure(error) || error.code !== "spotify_running") throw error;
      const ok = await ask({
        title: tr("Spotify is open", "Spotify est ouvert"),
        body: tr(
          "It has to be closed first. It reopens when it is done.",
          "Il doit être fermé d'abord. Il se rouvre une fois terminé.",
        ),
        confirm: tr("Close Spotify", "Fermer Spotify"),
        cancel: tr("Cancel", "Annuler"),
      });
      if (!ok) return;
      done = await withBusy(label, () => call(true));
    }
    toast("good", done.message, { detail: done.detail });
  } catch (error) {
    if (isFailure(error) && error.code === "spicetify") {
      await ask({
        title: tr("Spicetify is active", "Spicetify est actif"),
        body: error.message,
        confirm: "OK",
        info: true,
      });
    } else {
      showError(error);
    }
  } finally {
    await refreshOverview();
  }
}

/** A message after a change that shows in Spotify only once applied. */
export function changedToast(text: string, detail?: string | null) {
  toast("good", text, {
    detail,
    action: { label: tr("Apply", "Appliquer"), run: () => runAction("apply") },
  });
}
