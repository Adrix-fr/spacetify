<script lang="ts">
  // The steps of an install or uninstall, ticked as they finish.
  import { Check, X } from "@lucide/svelte";
  import type { Step, StepState } from "./api";
  import { stepLabel } from "./texts";

  let {
    steps,
    states,
    notes = {},
  }: {
    steps: Step[];
    states: Partial<Record<Step, StepState>>;
    notes?: Partial<Record<Step, string>>;
  } = $props();
</script>

<ul>
  {#each steps as step (step)}
    {@const state = states[step]}
    <li class={state ?? "waiting"}>
      <span class="icon" aria-hidden="true">
        {#if state === "done"}
          <Check size={16} strokeWidth={1.75} />
        {:else if state === "failed"}
          <span class="cross"><X size={11} strokeWidth={2} /></span>
        {:else if state === "active"}
          <span class="spinner"></span>
        {:else}
          <span class="ring"></span>
        {/if}
      </span>
      <span class="text">
        <span>{stepLabel(step)}</span>
        {#if notes[step]}
          <span class="note">{notes[step]}</span>
        {/if}
      </span>
    </li>
  {/each}
</ul>

<style>
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 10px;
    font-size: 13px;
  }

  li {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    color: var(--text-3);
  }

  li.done {
    color: var(--text-2);
  }

  li.active,
  li.failed {
    color: var(--text);
    font-weight: 500;
  }

  .icon {
    width: 16px;
    height: 20px;
    flex: none;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--accent);
  }

  .text {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .note {
    font-size: 12px;
    line-height: 18px;
    font-weight: 400;
    color: var(--text-3);
  }

  .ring,
  .spinner,
  .cross {
    width: 13px;
    height: 13px;
    box-sizing: border-box;
    border-radius: 50%;
  }

  .ring {
    border: 1.4px solid #3a3a3f;
  }

  .spinner {
    border: 1.6px solid #2a2a2e;
    border-top-color: var(--accent);
    animation: spin 800ms linear infinite;
  }

  .cross {
    width: 15px;
    height: 15px;
    display: flex;
    align-items: center;
    justify-content: center;
    border: 1.4px solid var(--warn);
    color: var(--warn);
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .spinner {
      animation-duration: 2400ms;
    }
  }
</style>
