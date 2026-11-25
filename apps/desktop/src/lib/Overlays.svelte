<script lang="ts">
  // Progress, confirmation dialog and messages, above every page.
  import { X, Check, CircleAlert, Info } from "@lucide/svelte";
  import { app, answer, dialog, dismiss, toasts, tr } from "./state.svelte";

  function onkeydown(event: KeyboardEvent) {
    if (dialog.current && event.key === "Escape") answer(false);
  }
</script>

<svelte:window {onkeydown} />

{#if app.busy}
  <div class="veil" role="status" aria-live="polite">
    <div class="busy">
      <span class="spinner" aria-hidden="true"></span>
      <span>{app.busy}</span>
    </div>
  </div>
{/if}

{#if dialog.current}
  {@const q = dialog.current}
  <div class="veil">
    <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="dialog-title">
      <h2 id="dialog-title">{q.title}</h2>
      <p>{q.body}</p>
      <div class="actions">
        {#if !q.info}
          <button class="btn" onclick={() => answer(false)}>{q.cancel ?? tr("Cancel", "Annuler")}</button>
        {/if}
        <!-- svelte-ignore a11y_autofocus -->
        <button class="btn primary" autofocus onclick={() => answer(true)}>{q.confirm}</button>
      </div>
    </div>
  </div>
{/if}

<div class="toasts" aria-live="polite">
  {#each toasts as t (t.id)}
    <div class="toast {t.tone}">
      <span class="icon">
        {#if t.tone === "good"}<Check size={16} strokeWidth={1.5} />{:else if t.tone === "bad"}<CircleAlert
            size={16}
            strokeWidth={1.5}
          />{:else}<Info size={16} strokeWidth={1.5} />{/if}
      </span>
      <div class="body">
        <p>{t.text}</p>
        {#if t.detail}<p class="sub">{t.detail}</p>{/if}
        {#if t.action}
          <button
            class="link"
            onclick={() => {
              t.action?.run();
              dismiss(t.id);
            }}>{t.action.label}</button
          >
        {/if}
      </div>
      <button class="close" aria-label={tr("Close", "Fermer")} onclick={() => dismiss(t.id)}
        ><X size={14} strokeWidth={1.5} /></button
      >
    </div>
  {/each}
</div>

<style>
  .veil {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(14, 14, 15, 0.7);
  }
  .busy {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 16px;
    border-radius: var(--r-card);
    border: 1px solid var(--line);
    background: var(--surface);
  }
  .spinner {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    border: 2px solid var(--line-strong);
    border-top-color: var(--accent);
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  .dialog {
    width: min(420px, calc(100vw - 32px));
    padding: 20px;
    border-radius: 14px;
    border: 1px solid var(--line);
    background: var(--surface);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .dialog h2 {
    font-size: 15px;
    font-weight: 600;
  }
  .dialog p {
    color: var(--text-2);
    white-space: pre-line;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 12px;
  }
  .toasts {
    position: fixed;
    right: 16px;
    bottom: 16px;
    z-index: 60;
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: min(360px, calc(100vw - 32px));
  }
  .toast {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    padding: 12px;
    border-radius: var(--r-card);
    border: 1px solid var(--line);
    background: var(--raised);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
  }
  .toast .body {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 4px;
    white-space: pre-line;
  }
  .icon {
    display: flex;
    margin-top: 2px;
    color: var(--text-3);
  }
  .good .icon {
    color: var(--good);
  }
  .bad .icon {
    color: var(--bad);
  }
  .link {
    align-self: flex-start;
    border: 0;
    padding: 0;
    background: none;
    color: var(--accent);
    font-weight: 500;
    cursor: pointer;
  }
  .close {
    display: flex;
    border: 0;
    background: none;
    color: var(--text-3);
    cursor: pointer;
    padding: 2px;
  }
</style>
