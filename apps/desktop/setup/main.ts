import "@fontsource-variable/plus-jakarta-sans";
import "../src/app.css";
import { mount } from "svelte";
import Setup from "./Setup.svelte";

// In a plain browser during development, a fake Rust side plays the
// scenario named in the address (`?s=update`, `?s=diskfull`…, see mock.ts).
if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
  const { installMock } = await import("./mock");
  installMock();
}

const app = mount(Setup, { target: document.getElementById("app")! });

export default app;
