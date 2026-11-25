import "@fontsource-variable/plus-jakarta-sans";
import "./app.css";
import { mount } from "svelte";
import App from "./App.svelte";

// In a plain browser during development, sample data stands in for the
// Rust side. The built app never includes this.
if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window)) {
  const { installMock } = await import("./dev/mock");
  installMock();
}

const app = mount(App, { target: document.getElementById("app")! });

export default app;
