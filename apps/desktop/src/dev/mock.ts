// Development only: lets `npm run dev` show the window in a normal browser,
// with sample data instead of the Rust side. Never part of the built app
// (see `main.ts`). Handy to work on the look without touching Spotify.
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";

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
  mockWindows("main");
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
          themes: [{ id: "midnight", name: "Midnight", version: "1.0.0", author: "spacetify", description: "Deep blue backgrounds with a violet accent.", active: true, error: null, palette: midnight, settings: [], verified: true }],
          extensions: [
            {
              id: "hello-badge", name: "Hello badge", version: "1.0.0", author: "spacetify", description: "Shows a small badge above the player bar.", active: extOn, error: null, palette: null, verified: false,
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
      case "submit_candidates":
        return [
          { folder: "C:\\Users\\moi\\mods\\aurora", id: "aurora", name: "Aurora", kind: "theme", version: "1.0.0", palette: midnight, inGallery: "no" },
          { folder: "C:\\Users\\moi\\mods\\lyrics", id: "lyrics", name: "Paroles plein écran", kind: "extension", version: "0.3.0", palette: null, inGallery: "older" },
        ];
      case "submit_review": {
        const theme = (args as { folder: string }).folder.endsWith("aurora");
        const ok = (title: string, clean: string) => ({ title, level: "ok", clean, findings: [] });
        return {
          candidate: null,
          inGallery: theme ? "no" : "older",
          galleryVersion: theme ? null : "0.2.0",
          summary: theme ? "7 contrôles bons · 0 à regarder · 0 bloquant(s)" : "6 contrôles bons · 1 à regarder · 0 bloquant(s)",
          blocking: 0,
          toLookAt: theme ? 0 : 1,
          size: theme ? 18_400 : 9_100,
          checks: [
            ok("Mod valide", "Manifeste et fichiers corrects"),
            ok("Programmes déguisés", "Seulement des fichiers permis"),
            ok("Caractères invisibles", "Aucun caractère invisible"),
            ok("Code brouillé", "Code lisible"),
            theme
              ? ok("Adresses internet", "Aucune adresse internet")
              : {
                  title: "Adresses internet",
                  level: "look",
                  clean: null,
                  findings: [{ what: "Contacte https://lrclib.net/api/get.", place: "main.js, ligne 42", excerpt: "const res = await fetch(`https://lrclib.net/api/get?${q}`);", level: "look" }],
                },
            ok("Accès sensibles", "Aucun accès sensible"),
            ok("Images piégées", "Images saines"),
          ],
        };
      }
      case "submit_prepare":
        return { zip: "C:\\Users\\moi\\Downloads\\aurora-1.0.0.zip", formUrl: "https://github.com/spacetify-mod/gallery/issues/new" };
      case "take_start_page":
        return null;
      case "spicetify_plan":
        return {
          offered: true,
          blocked: null,
          steps: [
            { label: "Spotify", detail: "Retirer les 2 dossier(s) laissé(s) par Spicetify dans Spotify", tone: "good" },
            { label: "Sleek · Coral", detail: "Couleurs importées, et choisi comme thème actif", tone: "good" },
            { label: "fullAppDisplay.js", detail: "Remplacée par « fullscreen » de la galerie, installée pour toi", tone: "good" },
            { label: "adblock.js", detail: "Pas encore d'équivalent", tone: "none" },
            { label: "marketplace", detail: "Remplacé par la galerie spacetify, intégrée à Spotify", tone: "good" },
          ],
        };
      case "leave_spicetify":
        return {
          message: "Tu as quitté Spicetify. Tes mods ne disparaissent plus après les mises à jour de Spotify.",
          detail: "1 thème(s) importé(s) · 1 extension(s) installée(s).",
        };
      default:
        return null;
    }
  });
}
