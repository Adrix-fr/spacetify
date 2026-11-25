<script lang="ts">
  import { onMount, tick } from "svelte";
  import { Square, CornerDownLeft } from "@lucide/svelte";
  import { api, events, failureMessage } from "../lib/api";
  import { refreshOverview, tr } from "../lib/state.svelte";
  import Toggle from "../lib/Toggle.svelte";

  interface Row {
    id: number;
    kind: "cmd" | "out" | "err" | "info" | "event";
    text: string;
  }

  // Kept while the window is open, so switching tabs does not lose it.
  const memory = (globalThis as unknown as { __terminal?: { rows: Row[]; history: string[]; seen: string[]; follow: boolean } })
    .__terminal ?? { rows: [], history: [], seen: [], follow: true };
  (globalThis as unknown as { __terminal: typeof memory }).__terminal = memory;

  let rows = $state<Row[]>(memory.rows);
  let line = $state("");
  let running = $state(false);
  let follow = $state(memory.follow);
  let historyIndex = -1;
  let nextId = rows.length + 1;
  let output: HTMLDivElement;
  const seen = new Set(memory.seen);

  const shortcuts = ["status", "list", "doctor", "service status", "service events", "--help"];

  function push(kind: Row["kind"], text: string) {
    rows.push({ id: nextId++, kind, text });
    if (rows.length > 2000) rows.splice(0, rows.length - 2000);
    memory.rows = rows;
    tick().then(() => output?.scrollTo({ top: output.scrollHeight }));
  }

  async function run(command: string) {
    const text = command.trim();
    if (running) return;
    push("cmd", `spacetify ${text}`.trim());
    if (text) {
      memory.history = [text, ...memory.history.filter((h) => h !== text)].slice(0, 50);
    }
    historyIndex = -1;
    line = "";
    try {
      running = true;
      await api.runCli(text);
    } catch (error) {
      running = false;
      push("err", failureMessage(error));
    }
  }

  function onkeydown(event: KeyboardEvent) {
    if (event.key === "Enter") {
      run(line);
    } else if (event.key === "ArrowUp" && memory.history.length > 0) {
      historyIndex = Math.min(historyIndex + 1, memory.history.length - 1);
      line = memory.history[historyIndex];
      event.preventDefault();
    } else if (event.key === "ArrowDown") {
      historyIndex = Math.max(historyIndex - 1, -1);
      line = historyIndex >= 0 ? memory.history[historyIndex] : "";
      event.preventDefault();
    }
  }

  /** Adds the service's new events to the log, as they happen. */
  async function pollEvents(first: boolean) {
    if (!follow) return;
    try {
      const info = await api.repairInfo(first ? 10 : 20);
      for (const event of [...info.events].reverse()) {
        const key = event.at + event.text;
        if (seen.has(key)) continue;
        seen.add(key);
        memory.seen = [...seen].slice(-500);
        push("event", `${new Date(event.at).toLocaleTimeString()}  ${event.text}`);
      }
    } catch {
      // The log is a convenience: ignore a failed read.
    }
  }

  function setFollow(on: boolean) {
    follow = on;
    memory.follow = on;
    if (on) pollEvents(true);
  }

  onMount(() => {
    if (rows.length === 0) {
      push(
        "info",
        tr(
          "Type a command, for example status or --help",
          "Tape une commande, par exemple status ou --help",
        ),
      );
    }
    const unlisten = [
      events.terminalLine((l) => push(l.stream, l.text)),
      events.terminalExit((exit) => {
        running = false;
        if (exit.code !== 0) push("info", tr(`exit code ${exit.code ?? "?"}`, `code de sortie ${exit.code ?? "?"}`));
        refreshOverview();
      }),
    ];
    pollEvents(true);
    const timer = setInterval(() => {
      if (!document.hidden) pollEvents(false);
    }, 3000);
    tick().then(() => output?.scrollTo({ top: output.scrollHeight }));
    return () => {
      clearInterval(timer);
      unlisten.forEach((p) => p.then((stop) => stop()));
    };
  });
</script>

<div class="page">
  <section class="page-head">
    <div>
      <h1>Terminal</h1>
      <p>
        {tr(
          "The spacetify commands, also available in any terminal",
          "Les commandes spacetify, aussi disponibles dans n'importe quel terminal",
        )}
      </p>
    </div>
    <label class="follow">
      <span class="sub">{tr("Service activity", "Activité du service")}</span>
      <Toggle checked={follow} label={tr("Service activity", "Activité du service")} onchange={setFollow} />
    </label>
  </section>

  <section class="term card">
    <div class="output scroll" bind:this={output} role="log" aria-live="polite">
      {#each rows as row (row.id)}
        <div class="line {row.kind}">{row.text}</div>
      {/each}
    </div>
    <div class="prompt">
      <span class="ps">spacetify</span>
      <!-- svelte-ignore a11y_autofocus -->
      <input
        type="text"
        bind:value={line}
        {onkeydown}
        autofocus
        spellcheck="false"
        autocomplete="off"
        aria-label={tr("Command", "Commande")}
        placeholder={running ? tr("Running…", "En cours…") : "status"}
        disabled={running}
      />
      {#if running}
        <button class="btn" onclick={() => api.stopCli()}><Square size={14} strokeWidth={1.5} />{tr("Stop", "Arrêter")}</button>
      {:else}
        <button class="btn" onclick={() => run(line)}><CornerDownLeft size={14} strokeWidth={1.5} />{tr("Run", "Lancer")}</button>
      {/if}
    </div>
  </section>

  <div class="chips">
    {#each shortcuts as shortcut (shortcut)}
      <button class="chip" disabled={running} onclick={() => run(shortcut)}>{shortcut}</button>
    {/each}
  </div>
</div>

<style>
  .follow {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .term {
    overflow: hidden;
  }
  .output {
    height: min(52vh, 460px);
    padding: 12px 16px;
    background: var(--bg);
    font-family: var(--mono);
    font-size: 12px;
    line-height: 18px;
  }
  .line {
    white-space: pre-wrap;
    word-break: break-word;
    color: var(--text-2);
  }
  .line.cmd {
    color: var(--text);
    margin-top: 8px;
  }
  .line.cmd::before {
    content: "› ";
    color: var(--accent);
  }
  .line.err {
    color: var(--bad);
  }
  .line.info {
    color: var(--text-3);
  }
  .line.event {
    color: var(--text-2);
  }
  .line.event::before {
    content: "● ";
    color: var(--accent);
  }
  .prompt {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 8px 8px 16px;
    border-top: 1px solid var(--line);
  }
  .ps {
    font-family: var(--mono);
    font-size: 12px;
    color: var(--accent);
  }
  .prompt input {
    flex: 1;
    border: 0;
    background: transparent;
    font-family: var(--mono);
    font-size: 13px;
    height: 32px;
    padding: 0 4px;
  }
  .prompt input:focus {
    outline: none;
  }
  .chips {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .chip {
    height: 24px;
    padding: 0 8px;
    border-radius: 6px;
    border: 1px solid var(--line-strong);
    background: transparent;
    color: var(--text-2);
    font-family: var(--mono);
    font-size: 12px;
    cursor: pointer;
    transition:
      color 150ms var(--ease),
      border-color 150ms var(--ease);
  }
  .chip:hover:not(:disabled) {
    color: var(--text);
    border-color: var(--text-3);
  }
  .chip:disabled {
    opacity: 0.4;
  }
</style>
