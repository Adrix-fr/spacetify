<script lang="ts">
  let {
    checked,
    label,
    disabled = false,
    onchange,
  }: { checked: boolean; label: string; disabled?: boolean; onchange: (value: boolean) => void } = $props();
</script>

<button
  type="button"
  role="switch"
  class="toggle"
  class:on={checked}
  aria-checked={checked}
  aria-label={label}
  {disabled}
  onclick={() => onchange(!checked)}
>
  <span class="knob"></span>
</button>

<style>
  .toggle {
    flex: none;
    width: 32px;
    height: 20px;
    border-radius: 10px;
    border: 0;
    padding: 2px;
    background: var(--line-strong);
    cursor: pointer;
    transition: background-color 150ms var(--ease);
  }
  .toggle.on {
    background: var(--accent);
  }
  .toggle:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .knob {
    display: block;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--text);
    transition: transform 150ms var(--ease);
  }
  .on .knob {
    transform: translateX(12px);
  }
</style>
