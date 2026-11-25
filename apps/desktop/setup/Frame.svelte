<script lang="ts">
  // The installer window: its own title bar (the system one is hidden), the
  // screen, and a footer for the buttons.
  import type { Snippet } from "svelte";
  import { Minus, X } from "@lucide/svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import Logo from "../src/lib/Logo.svelte";
  import { tr } from "./i18n.svelte";

  let {
    title,
    canClose = true,
    onclose,
    bare = false,
    children,
    footer,
    overlay,
  }: {
    title: string;
    canClose?: boolean;
    onclose: () => void;
    /** No footer (while working). */
    bare?: boolean;
    children: Snippet;
    footer?: Snippet;
    overlay?: Snippet;
  } = $props();
</script>

<div class="window">
  <div class="titlebar" data-tauri-drag-region>
    <span class="brand"><Logo size={16} /></span>
    <span class="title">{title}</span>
    <button
      class="control"
      aria-label={tr("Minimize", "Réduire")}
      onclick={() => getCurrentWindow().minimize()}
    >
      <Minus size={14} strokeWidth={1.5} />
    </button>
    <button
      class="control close"
      aria-label={tr("Close", "Fermer")}
      disabled={!canClose}
      onclick={onclose}
    >
      <X size={14} strokeWidth={1.5} />
    </button>
  </div>

  <main class="body">
    {@render children()}
  </main>

  {#if footer && !bare}
    <footer>
      {@render footer()}
    </footer>
  {/if}

  {#if overlay}
    {@render overlay()}
  {/if}
</div>

<style>
  .window {
    position: relative;
    height: 100vh;
    display: flex;
    flex-direction: column;
    background: var(--bg);
    overflow: hidden;
  }

  .titlebar {
    height: 40px;
    flex: none;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 4px 0 14px;
    border-bottom: 1px solid rgba(237, 237, 237, 0.06);
    user-select: none;
  }

  /* Drags go to the bar itself. */
  .brand,
  .title {
    pointer-events: none;
    display: flex;
  }

  .title {
    flex: 1;
    font-size: 12px;
    color: var(--text-3);
  }

  .control {
    width: 44px;
    height: 32px;
    display: flex;
    align-items: center;
    justify-content: center;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--text-2);
    cursor: pointer;
    transition: background-color 150ms var(--ease);
  }

  .control:hover:not(:disabled) {
    background: var(--raised);
    color: var(--text);
  }

  .control.close:hover:not(:disabled) {
    background: #c42b1c;
    color: #fff;
  }

  .control:disabled {
    opacity: 0.35;
    cursor: default;
  }

  .body {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  footer {
    flex: none;
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    min-height: 68px;
    box-sizing: border-box;
    padding: 14px 16px 14px 20px;
    border-top: 1px solid rgba(237, 237, 237, 0.06);
    background: #121214;
  }

  footer :global(.spacer) {
    flex: 1;
  }

  footer :global(.link),
  .body :global(.link) {
    border: 0;
    padding: 0;
    background: none;
    color: var(--accent);
    font: inherit;
    font-size: 13px;
    cursor: pointer;
  }

  footer :global(.link:hover),
  .body :global(.link:hover) {
    color: #ffa273;
  }

  footer :global(.btn) {
    height: 40px;
    padding: 0 16px;
  }

  footer :global(.btn.primary) {
    padding: 0 22px;
    font-weight: 700;
  }

  footer :global(.btn.destructive) {
    border-color: transparent;
    background: var(--bad);
    color: var(--on-accent);
    font-weight: 700;
  }

  footer :global(.btn.destructive:hover:not(:disabled)) {
    background: #eb5e62;
  }
</style>
