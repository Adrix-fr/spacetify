<script lang="ts">
  import { onMount } from "svelte";
  import { Settings2 } from "@lucide/svelte";
  import { api, events } from "./lib/api";
  import Logo from "./lib/Logo.svelte";
  import Overlays from "./lib/Overlays.svelte";
  import { app, refreshOverview, runAction, tr, type Page } from "./lib/state.svelte";
  import Home from "./pages/Home.svelte";
  import Themes from "./pages/Themes.svelte";
  import Extensions from "./pages/Extensions.svelte";
  import Compat from "./pages/Compat.svelte";
  import Repair from "./pages/Repair.svelte";
  import Terminal from "./pages/Terminal.svelte";
  import Settings from "./pages/Settings.svelte";

  const tabs: { page: Page; label: () => string }[] = [
    { page: "home", label: () => tr("Home", "Accueil") },
    { page: "themes", label: () => tr("Themes", "Thèmes") },
    { page: "extensions", label: () => "Extensions" },
    { page: "compat", label: () => tr("Compatibility", "Compatibilité") },
    { page: "repair", label: () => tr("Auto repair", "Réparation auto") },
    { page: "terminal", label: () => "Terminal" },
  ];

  let ready = $state(false);

  onMount(() => {
    // Focus rings only for keyboard users: on with Tab, off with the mouse.
    const root = document.documentElement;
    const keyboard = (e: KeyboardEvent) => {
      if (e.key === "Tab") root.dataset.keyboard = "";
    };
    const pointer = () => delete root.dataset.keyboard;
    window.addEventListener("keydown", keyboard);
    window.addEventListener("pointerdown", pointer);

    const unlisten = [
      events.progress((text) => {
        if (app.busy) app.busy = text;
      }),
      events.trayAction((action) => {
        app.page = "home";
        runAction(action);
      }),
    ];
    (async () => {
      try {
        app.lang = (await api.getSettings()).lang;
      } catch {
        // English if the settings cannot be read.
      }
      document.documentElement.lang = app.lang;
      ready = true;
      await refreshOverview();
    })();
    // Keeps the status fresh while the window is shown.
    const timer = setInterval(() => {
      if (!document.hidden && !app.busy) refreshOverview();
    }, 5000);
    return () => {
      window.removeEventListener("keydown", keyboard);
      window.removeEventListener("pointerdown", pointer);
      clearInterval(timer);
      unlisten.forEach((p) => p.then((stop) => stop()));
    };
  });
</script>

<div class="shell">
  <header>
    <div class="brand">
      <Logo size={22} />
      <span>spacetify</span>
    </div>
    <nav aria-label={tr("Sections", "Rubriques")}>
      {#each tabs as tab (tab.page)}
        <button
          class="tab"
          class:active={app.page === tab.page}
          aria-current={app.page === tab.page ? "page" : undefined}
          onclick={() => (app.page = tab.page)}>{tab.label()}</button
        >
      {/each}
    </nav>
    <button
      class="tab icon"
      class:active={app.page === "settings"}
      aria-label={tr("Settings", "Réglages")}
      title={tr("Settings", "Réglages")}
      onclick={() => (app.page = "settings")}
    >
      <Settings2 size={16} strokeWidth={1.5} />
    </button>
  </header>

  <main class="scroll">
    {#if ready}
      {#key app.page}
        <div class="content enter">
          {#if app.page === "home"}
            <Home />
          {:else if app.page === "themes"}
            <Themes />
          {:else if app.page === "extensions"}
            <Extensions />
          {:else if app.page === "compat"}
            <Compat />
          {:else if app.page === "repair"}
            <Repair />
          {:else if app.page === "terminal"}
            <Terminal />
          {:else}
            <Settings />
          {/if}
        </div>
      {/key}
    {/if}
  </main>
</div>

<Overlays />

<style>
  .shell {
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  header {
    display: flex;
    align-items: center;
    gap: 24px;
    height: 56px;
    padding: 0 20px;
    border-bottom: 1px solid var(--line);
    flex: none;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 15px;
    font-weight: 600;
  }
  nav {
    display: flex;
    gap: 4px;
    min-width: 0;
    overflow-x: auto;
    scrollbar-width: none;
  }
  .tab {
    height: 32px;
    padding: 0 12px;
    border: 0;
    border-radius: var(--r-control);
    background: transparent;
    color: var(--text-2);
    font-size: 13px;
    font-weight: 500;
    white-space: nowrap;
    cursor: pointer;
    transition:
      background-color 150ms var(--ease),
      color 150ms var(--ease);
  }
  .tab:hover {
    color: var(--text);
  }
  .tab.active {
    background: var(--raised);
    color: var(--text);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.06);
  }
  :global(html[data-keyboard]) .tab:focus-visible {
    box-shadow: 0 0 0 2px rgba(255, 107, 129, 0.45);
  }
  .tab.icon {
    margin-left: auto;
    width: 32px;
    padding: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    flex: none;
  }
  main {
    flex: 1;
    min-height: 0;
  }
  .content {
    max-width: 1040px;
    margin: 0 auto;
    padding: 32px 32px 48px;
  }
</style>
