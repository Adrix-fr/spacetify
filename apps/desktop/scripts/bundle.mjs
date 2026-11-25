// Builds the Windows installer: `npm run bundle`.
//
// 1. the `spacetify` and `spacetify-service` programs, and the app
//    (`spacetify-app`, with its web files);
// 2. the installer's window, then `uninstall.exe` (the installer program
//    built without anything to install);
// 3. the files to install, gathered in `<target>/setup-payload` as they are
//    laid out in the install folder;
// 4. the installer, with those files inside:
//    `<target>/release/bundle/setup/spacetify_<version>_x64-setup.exe`.
//
// `<target>` is `target`, or `CARGO_TARGET_DIR` when it is set.
import { execSync } from "node:child_process";
import { copyFileSync, cpSync, mkdirSync, readFileSync, rmSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

if (process.platform !== "win32") {
  throw new Error("the installer is built on Windows");
}

const here = dirname(fileURLToPath(import.meta.url));
const desktop = resolve(here, "..");
const root = resolve(desktop, "../..");
const targetDir = resolve(root, process.env.CARGO_TARGET_DIR ?? "target");
const release = join(targetDir, "release");
const version = JSON.parse(readFileSync(join(desktop, "package.json"), "utf8")).version;

// Through the shell, so `npm` and `npx` (.cmd files) are found. The
// arguments are fixed words, never paths.
function run(command, args, options = {}) {
  const line = [command, ...args].join(" ");
  console.log(`\n> ${line}`);
  execSync(line, { stdio: "inherit", ...options });
}

function cargo(args, env = {}) {
  run("cargo", args, { cwd: root, env: { ...process.env, ...env } });
}

// 1. The programs.
cargo(["build", "--release", "--locked", "-p", "spacetify-cli", "-p", "spacetify-service"]);
run("npx", ["tauri", "build", "--no-bundle"], { cwd: desktop });

// 2. The installer's window, and the uninstaller.
run("npm", ["run", "build:setup"], { cwd: desktop });
cargo(["build", "--release", "--locked", "-p", "spacetify-setup"], {
  SPACETIFY_SETUP_PAYLOAD: "",
});

// 3. The files to install.
const payload = join(targetDir, "setup-payload");
rmSync(payload, { recursive: true, force: true });
mkdirSync(payload, { recursive: true });
for (const name of ["spacetify-app.exe", "spacetify.exe", "spacetify-service.exe"]) {
  copyFileSync(join(release, name), join(payload, name));
}
copyFileSync(join(release, "spacetify-setup.exe"), join(payload, "uninstall.exe"));
copyFileSync(join(desktop, "src-tauri/icons/notification.png"), join(payload, "icon.png"));
cpSync(join(root, "examples"), join(payload, "examples"), { recursive: true });

// 4. The installer.
cargo(["build", "--release", "--locked", "-p", "spacetify-setup"], {
  SPACETIFY_SETUP_PAYLOAD: payload,
});
const out = join(release, "bundle", "setup");
mkdirSync(out, { recursive: true });
const installer = join(out, `spacetify_${version}_x64-setup.exe`);
copyFileSync(join(release, "spacetify-setup.exe"), installer);
console.log(`\nInstaller: ${installer}`);
