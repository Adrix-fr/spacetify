// Builds the macOS and Linux packages: `npm run bundle` (bundle.mjs) runs
// this file on those systems. Windows has its own installer (bundle.mjs).
//
// Here spacetify ships in each system's usual format, made by Tauri:
//
// - macOS: `spacetify.app` in a `.dmg`, one universal file for Apple
//   Silicon and Intel Macs, signed ad hoc (no Apple account), in
//   `<target>/universal-apple-darwin/release/bundle/dmg/`;
// - Linux: a `.deb` and an `.rpm`, in `<target>/release/bundle/{deb,rpm}/`.
//
// Steps:
// 1. the `spacetify` and `spacetify-service` programs (on macOS, once per
//    processor, then joined with `lipo`), copied to `<target>/sidecars` under
//    the names Tauri expects (`<name>-<target triple>`, the triple being
//    `universal-apple-darwin` for the joined ones);
// 2. the app (`spacetify-app`, with its web files) and the package, with the
//    two programs next to the app and the example mods in its resources.
//
// The packaging settings are passed with `--config` rather than written in
// `tauri.conf.json`: there, `externalBin` would make every `cargo build` of
// the app need the two programs first.
//
// `<target>` is `target`, or `CARGO_TARGET_DIR` when it is set.
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const desktop = resolve(here, "..");
const root = resolve(desktop, "../..");
const targetDir = resolve(root, process.env.CARGO_TARGET_DIR ?? "target");
const mac = process.platform === "darwin";

if (!mac && process.platform !== "linux") {
  throw new Error(`no package for ${process.platform}`);
}

// Without a shell: the arguments may be paths.
function run(command, args, options = {}) {
  console.log(`\n> ${[command, ...args].join(" ")}`);
  execFileSync(command, args, { stdio: "inherit", ...options });
}

const programs = ["spacetify", "spacetify-service"];
const crates = ["-p", "spacetify-cli", "-p", "spacetify-service"];
const sidecars = join(targetDir, "sidecars");
rmSync(sidecars, { recursive: true, force: true });
mkdirSync(sidecars, { recursive: true });

// 1. The programs.
let triple;
let tauriArgs;
if (mac) {
  triple = "universal-apple-darwin";
  const arches = ["aarch64-apple-darwin", "x86_64-apple-darwin"];
  for (const arch of arches) {
    run("cargo", ["build", "--release", "--locked", ...crates, "--target", arch], { cwd: root });
  }
  for (const name of programs) {
    const parts = arches.map((arch) => join(targetDir, arch, "release", name));
    run("lipo", ["-create", "-output", join(sidecars, `${name}-${triple}`), ...parts]);
    // The app is compiled once per processor, and each build looks for its
    // own copy; the package takes the universal one.
    arches.forEach((arch, i) => copyFileSync(parts[i], join(sidecars, `${name}-${arch}`)));
  }
  tauriArgs = ["--target", triple];
} else {
  triple = execFileSync("rustc", ["--print", "host-tuple"], { encoding: "utf8" }).trim();
  run("cargo", ["build", "--release", "--locked", ...crates], { cwd: root });
  for (const name of programs) {
    copyFileSync(join(targetDir, "release", name), join(sidecars, `${name}-${triple}`));
  }
  tauriArgs = [];
}

// 2. The app and the package.
const config = {
  bundle: {
    active: true,
    targets: mac ? ["app", "dmg"] : ["deb", "rpm"],
    externalBin: programs.map((name) => join(sidecars, name)),
    macOS: {
      // Ad hoc: signed, but by no one Apple knows. Without any signature,
      // macOS says a downloaded app "is damaged" and offers no way to open it.
      signingIdentity: "-",
    },
    linux: {
      // `notify-send`, which the service uses for its notifications.
      deb: { recommends: ["libnotify-bin"] },
      rpm: { recommends: ["libnotify"] },
    },
  },
};
const configFile = join(targetDir, "bundle-config.json");
writeFileSync(configFile, JSON.stringify(config, null, 2));
run("npx", ["tauri", "build", ...tauriArgs, "--config", configFile], { cwd: desktop });

const out = mac
  ? join(targetDir, triple, "release", "bundle", "dmg")
  : join(targetDir, "release", "bundle");
console.log(`\nPackages: ${out}`);
