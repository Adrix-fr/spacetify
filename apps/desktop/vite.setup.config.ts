import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// The installer's window (`apps/setup`). It shares the app's styles and
// components, so it lives next to them.
export default defineConfig({
  root: "setup",
  plugins: [svelte({ configFile: fileURLToPath(new URL("./svelte.config.js", import.meta.url)) })],
  clearScreen: false,
  server: { port: 1421, strictPort: true },
  build: { target: "es2022", outDir: "../dist-setup", emptyOutDir: true },
});
