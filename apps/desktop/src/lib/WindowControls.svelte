<script lang="ts">
  // Minimize, maximize and close, in the header: on Windows the system title
  // bar is hidden (`src-tauri/tauri.windows.conf.json`). Closing keeps the
  // app in the notification area, as before.
  import { onMount } from "svelte";
  import { Copy, Minus, Square, X } from "@lucide/svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { tr } from "./state.svelte";

  const win = getCurrentWindow();
  let maximized = $state(false);

  onMount(() => {
    const update = async () => {
      maximized = await win.isMaximized();
    };
    update();
    const stop = win.onResized(update);
    return () => {
      stop.then((unlisten) => unlisten());
    };
  });
</script>

<div class="controls">
  <button aria-label={tr("Minimize", "Réduire")} title={tr("Minimize", "Réduire")} onclick={() => win.minimize()}>
    <Minus size={15} strokeWidth={1.5} />
  </button>
  <button
    aria-label={maximized ? tr("Restore", "Restaurer") : tr("Maximize", "Agrandir")}
    title={maximized ? tr("Restore", "Restaurer") : tr("Maximize", "Agrandir")}
    onclick={() => win.toggleMaximize()}
  >
    {#if maximized}
      <Copy size={13} strokeWidth={1.5} />
    {:else}
      <Square size={12} strokeWidth={1.5} />
    {/if}
  </button>
  <button class="close" aria-label={tr("Close", "Fermer")} title={tr("Close", "Fermer")} onclick={() => win.close()}>
    <X size={16} strokeWidth={1.5} />
  </button>
</div>

<style>
  .controls {
    display: flex;
    align-items: center;
    gap: 2px;
    flex: none;
  }

  button {
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
    transition:
      background-color 150ms var(--ease),
      color 150ms var(--ease);
  }

  button:hover {
    background: var(--raised);
    color: var(--text);
  }

  button.close:hover {
    background: #c42b1c;
    color: #fff;
  }

  /* The "restore" icon: two overlapping windows, like Windows draws it. */
  button :global(.lucide-copy) {
    transform: scaleX(-1);
  }
</style>
