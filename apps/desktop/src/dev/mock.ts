// Development only: lets `npm run dev` show the window in a normal browser,
// with sample data instead of the Rust side. Never part of the built app
// (see `main.ts`). Handy to work on the look without touching Spotify.
import { mockIPC } from "@tauri-apps/api/mocks";

const midnight = {
  frame: "#07070f",
  background: "#0f1020",
  highlight: "#1c1e36",
  elevated: "#1c1e36",
  text: "#a9abc9",
  accent: "#a78bfa",
  swatches: [
    { label: "Fond", color: "#0f1020" },
    { label: "Survol", color: "#1c1e36" },
    { label: "Texte secondaire", color: "#a9abc9" },
    { label: "Accent", color: "#a78bfa" },
    { label: "Cadre", color: "#07070f" },
    { label: "Boutons", color: "#a78bfa" },
  ],
};
const original = {
  frame: "#000000",
  background: "#121212",
  highlight: "#1f1f1f",
  elevated: "#2a2a2a",
  text: "#b3b3b3",
  accent: "#1ed760",
  swatches: [],
};
const service = {
  running: true,
  autostart: "on",
  autostartDetail: null,
  phase: "Surveille Spotify, mods en place",
  started: "il y a 3 h",
  lastRepair: "il y a 2 j",
  installed: true,
};
const now = Date.now();
const events = [
  { at: new Date(now - 2 * 86400e3).toISOString(), when: "il y a 2 j", text: "Mods remis en place pour Spotify 1.3.10 (thème : Midnight, extensions : 1).", tone: "good" },
  { at: new Date(now - 2 * 86400e3 - 60e3).toISOString(), when: "il y a 2 j", text: "Spotify a été mis à jour (version 1.3.10) et les mods ont été retirés.", tone: "warn" },
  { at: new Date(now - 3 * 3600e3).toISOString(), when: "il y a 3 h", text: "Service démarré (spacetify 0.4.0).", tone: "info" },
];
let extOn = true;

export function installMock() {
  mockIPC(async (cmd, args) => {
    await new Promise((r) => setTimeout(r, 150));
    switch (cmd) {
      case "get_settings":
        return { language: "system", lang: "fr", version: "0.4.0", configDir: "C:\\Users\\moi\\AppData\\Roaming\\spacetify", dataDir: "C:\\Users\\moi\\AppData\\Local\\spacetify" };
      case "overview":
        return {
          spotify: { found: true, error: null, path: "C:\\Users\\moi\\AppData\\Roaming\\Spotify", version: "1.3.10.835", kind: "standard", readOnly: false, state: "patched", stateLabel: "Modifié par spacetify", running: false, spicetifyLeftovers: null, updateReplaced: false, storeAlsoInstalled: false, backups: 2 },
          service,
          theme: { id: "midnight", name: "Midnight", palette: midnight },
          extensions: extOn ? ["Hello badge"] : [],
          modsError: null,
        };
      case "doctor":
        return {
          spotifyVersion: "1.3.10.835",
          verdict: "compatible",
          summary: "Tout fonctionne avec cette version de Spotify.",
          mods: [
            { kind: "Thème", title: "Midnight 1.0.0", verdict: "compatible", headline: "24 éléments vérifiés, tous présents", error: null, problems: [] },
            { kind: "Extension", title: "Hello badge 1.0.0", verdict: "compatible", headline: "1 élément vérifié, présent", error: null, problems: [] },
          ],
        };
      case "repair_info":
        return { service, events: events.slice(0, (args as { count?: number }).count ?? 30), log: "C:\\Users\\moi\\AppData\\Local\\spacetify\\service\\service.log" };
      case "list_mods":
        return {
          themes: [{ id: "midnight", name: "Midnight", version: "1.0.0", author: "Adrix-fr", description: "Deep blue backgrounds with a violet accent.", active: true, error: null, palette: midnight, settings: [] }],
          extensions: [
            {
              id: "hello-badge", name: "Hello badge", version: "1.0.0", author: "Adrix-fr", description: "Shows a small badge above the player bar.", active: extOn, error: null, palette: null,
              settings: [
                { name: "side", label: "Side of the window", kind: "choice", value: "right", default: "right", choices: ["left", "right"], custom: false },
                { name: "text", label: "Badge text", kind: "string", value: "spacetify ✓", default: "spacetify ✓", choices: [], custom: false },
              ],
            },
          ],
          examples: [],
          folder: "C:\\Users\\moi\\AppData\\Roaming\\spacetify",
          original,
        };
      case "set_extension":
        extOn = (args as { on: boolean }).on;
        return null;
      case "apply":
        if (!(args as { closeSpotify: boolean }).closeSpotify) throw { code: "spotify_running", message: "Spotify est ouvert." };
        return { message: "Mods appliqués.", detail: null };
      case "restore":
        return { message: "Spotify est revenu à son état d'origine (empreinte vérifiée).", detail: null };
      case "run_cli":
        return null;
      default:
        return null;
    }
  });
}
