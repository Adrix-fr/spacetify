// Development only: lets `npm run dev:setup` show the installer in a normal
// browser. The address picks what happens: `?s=` followed by
//   fresh (default), older, same, newer, unknown, uninstall, running,
//   diskfull, blocked, inuse, other, partial, restorefail
// and `&lang=en` for English. Never part of the built installer.
import { emit } from "@tauri-apps/api/event";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";

export function installMock() {
  const params = new URLSearchParams(location.search);
  const scenario = params.get("s") ?? "fresh";
  const lang = params.get("lang") === "en" ? "en" : "fr";
  const uninstaller = scenario === "uninstall" || scenario === "restorefail";
  let programs = scenario === "running" ? ["app", "terminal"] : [];
  let tries = 0;

  const relation =
    scenario === "older" || scenario === "newer" || scenario === "same" || scenario === "unknown"
      ? scenario
      : "fresh";
  const installedVersion =
    relation === "older" ? "0.3.0" : relation === "newer" ? "0.5.0" : relation === "same" ? "0.4.0" : null;

  const wait = (ms: number) => new Promise((done) => setTimeout(done, ms));

  async function play(uninstall: boolean, restore: boolean) {
    tries += 1;
    const firstTry = tries === 1;
    if (uninstall) {
      if (restore) {
        await emit("setup:step", { step: "restore", state: "active" });
        await wait(700);
        if (scenario === "restorefail" && firstTry) {
          await emit("setup:step", { step: "restore", state: "failed" });
          await emit("setup:done", {
            result: "failed",
            step: "restore",
            failure: { kind: "other", message: "Spotify could not be found." },
          });
          return;
        }
        await emit("setup:step", { step: "restore", state: "done" });
        await emit("setup:progress", 0.35);
      }
      for (const step of ["undo", "remove"]) {
        await emit("setup:step", { step, state: "active" });
        await wait(600);
        await emit("setup:step", { step, state: "done" });
        await emit("setup:progress", step === "undo" ? 0.65 : 1);
      }
      await emit("setup:done", { result: "done" });
      return;
    }

    await emit("setup:step", { step: "files", state: "active" });
    for (let share = 0; share <= 0.7; share += 0.05) {
      await emit("setup:progress", share);
      await wait(120);
      if (cancelled) {
        cancelled = false;
        await emit("setup:done", { result: "cancelled" });
        return;
      }
    }
    const failures: Record<string, object> = {
      diskfull: { kind: "diskFull" },
      blocked: { kind: "blocked", file: "spacetify-service.exe" },
      inuse: { kind: "inUse", file: "spacetify-app.exe" },
      other: { kind: "other", message: "C:\\Users\\me\\AppData\\Local\\spacetify: invalid data" },
    };
    if (failures[scenario] && firstTry) {
      await emit("setup:step", { step: "files", state: "failed" });
      await emit("setup:done", { result: "failed", step: "files", failure: failures[scenario] });
      return;
    }
    await emit("setup:committed", null);
    await emit("setup:step", { step: "files", state: "done" });
    for (const [step, share] of [
      ["path", 0.85],
      ["notifications", 0.93],
      ["autostart", 1],
    ] as const) {
      await emit("setup:step", { step, state: "active" });
      await wait(450);
      const fails = scenario === "partial" && step === "notifications";
      await emit("setup:step", { step, state: fails ? "failed" : "done" });
      await emit("setup:progress", share);
    }
    if (scenario === "partial") {
      await emit("setup:done", {
        result: "partial",
        failed: [{ step: "notifications", message: "Access denied (os error 5)" }],
      });
    } else {
      await emit("setup:done", { result: "done" });
    }
  }

  let cancelled = false;
  mockWindows("main");
  mockIPC(
    async (cmd, args) => {
      const payload = args as Record<string, unknown> | undefined;
      switch (cmd) {
        case "context":
          return {
            mode: uninstaller ? "uninstall" : "install",
            lang,
            version: "0.4.0",
            installedVersion,
            relation: uninstaller ? "same" : relation,
            dir: "C:\\Users\\me\\AppData\\Local\\spacetify",
          };
        case "running":
          return programs;
        case "close_running":
          await wait(800);
          programs = [];
          return true;
        case "start":
          setTimeout(() => play(Boolean(payload?.uninstall), Boolean(payload?.restore)), 50);
          return true;
        case "retry":
          await wait(700);
          return [];
        case "cancel":
          cancelled = true;
          return null;
        case "quit":
          document.body.innerHTML =
            '<p style="padding:24px;color:#85858b">(The installer closed.)</p>';
          return null;
        default:
          return null;
      }
    },
    { shouldMockEvents: true },
  );
}
