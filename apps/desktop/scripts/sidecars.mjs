// Builds the `spacetify` and `spacetify-service` programs and copies them
// where the Tauri bundler expects them (`src-tauri/binaries/<name>-<target>`),
// so the installer puts them next to the app. Run by `npm run bundle`.
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, "../../..");
const binaries = resolve(here, "../src-tauri/binaries");

const rustc = execFileSync("rustc", ["-vV"], { encoding: "utf8" });
const target = /^host: (\S+)$/m.exec(rustc)?.[1];
if (!target) throw new Error("cannot read the target triple from `rustc -vV`");
const ext = process.platform === "win32" ? ".exe" : "";
// Same folder as `tauri build`, which also follows CARGO_TARGET_DIR.
const targetDir = resolve(root, process.env.CARGO_TARGET_DIR ?? "target");

execFileSync(
  "cargo",
  ["build", "--release", "--locked", "-p", "spacetify-cli", "-p", "spacetify-service"],
  { cwd: root, stdio: "inherit" },
);

mkdirSync(binaries, { recursive: true });
for (const name of ["spacetify", "spacetify-service"]) {
  const from = join(targetDir, "release", name + ext);
  const to = join(binaries, `${name}-${target}${ext}`);
  copyFileSync(from, to);
  console.log(`${from} -> ${to}`);
}
