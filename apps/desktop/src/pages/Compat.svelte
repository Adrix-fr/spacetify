<script lang="ts">
  import { onMount } from "svelte";
  import { CircleX, TriangleAlert, CircleHelp, RefreshCw, CircleAlert } from "@lucide/svelte";
  import { api, failureMessage, type DoctorView, type Verdict } from "../lib/api";
  import { tr } from "../lib/state.svelte";

  let report = $state<DoctorView | null>(null);
  let error = $state<string | null>(null);
  let loading = $state(false);

  async function load() {
    loading = true;
    try {
      report = await api.doctor();
      error = null;
    } catch (e) {
      error = failureMessage(e);
    } finally {
      loading = false;
    }
  }

  onMount(load);

  function dot(verdict: Verdict) {
    if (verdict === "compatible") return "good";
    if (verdict === "warnings") return "warn";
    if (verdict === "nothing") return "";
    return "bad";
  }

  function word(verdict: Verdict) {
    switch (verdict) {
      case "nothing":
        return tr("Nothing to check", "Rien à vérifier");
      case "compatible":
        return tr("Compatible", "Compatible");
      case "warnings":
        return tr("Warnings", "Avertissements");
      case "invalid":
        return tr("Cannot be loaded", "Impossible à charger");
      default:
        return tr("Partly broken", "En partie cassé");
    }
  }
</script>

<div class="page">
  <section class="page-head">
    <div>
      <h1>{tr("Compatibility", "Compatibilité")}</h1>
      <p>
        {tr(
          "Checks, without opening Spotify, that what your mods use still exists",
          "Vérifie, sans ouvrir Spotify, que ce qu'utilisent tes mods existe encore",
        )}
      </p>
    </div>
    <button class="btn" onclick={load} disabled={loading}>
      <RefreshCw size={14} strokeWidth={1.5} />{tr("Check again", "Revérifier")}
    </button>
  </section>

  {#if error}
    <div class="notice bad"><CircleAlert size={16} strokeWidth={1.5} /><span>{error}</span></div>
  {:else if !report}
    <p class="sub">{tr("Checking…", "Vérification…")}</p>
  {:else}
    <section class="card summary">
      <span class="dot {dot(report.verdict)}"></span>
      <div class="text">
        <p class="name">{word(report.verdict)}</p>
        <p class="sub">{report.summary}</p>
      </div>
      {#if report.spotifyVersion}<span class="meta">Spotify {report.spotifyVersion}</span>{/if}
    </section>

    {#if report.mods.length > 0}
      <section class="card">
        {#each report.mods as mod (mod.kind + mod.title)}
          <article class="mod">
            <div class="mod-head">
              <span class="dot {dot(mod.verdict)}"></span>
              <p class="name">{mod.title}</p>
              <span class="meta">{mod.kind}</span>
            </div>
            <p class="sub indent">{mod.headline}</p>
            {#if mod.error}<p class="indent error">{mod.error}</p>{/if}
            {#if mod.problems.length > 0}
              <ul class="indent">
                {#each mod.problems as problem (problem.text)}
                  <li class={problem.level}>
                    {#if problem.level === "missing"}<CircleX size={14} strokeWidth={1.5} />{:else if problem.level === "fragile"}<TriangleAlert
                        size={14}
                        strokeWidth={1.5}
                      />{:else}<CircleHelp size={14} strokeWidth={1.5} />{/if}
                    <span>{problem.text}</span>
                  </li>
                {/each}
              </ul>
            {/if}
          </article>
        {/each}
      </section>
    {/if}

    <p class="meta">
      {tr(
        "After a Spotify update, the automatic repair runs this check and turns off mods that no longer fit.",
        "Après une mise à jour de Spotify, la réparation auto fait cette vérification et désactive les mods qui ne vont plus.",
      )}
    </p>
  {/if}
</div>

<style>
  .summary {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 16px;
  }
  .text {
    flex: 1;
    min-width: 0;
  }
  .name {
    font-weight: 500;
  }
  .mod {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 12px 16px;
  }
  .mod + .mod {
    border-top: 1px solid var(--line);
  }
  .mod-head {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .mod-head .name {
    flex: 1;
  }
  .indent {
    margin-left: 20px;
  }
  .error {
    color: var(--bad);
  }
  ul {
    margin-top: 4px;
    padding: 0;
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  li {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    font-family: var(--mono);
    font-size: 12px;
    line-height: 18px;
    color: var(--text-2);
    word-break: break-word;
  }
  li :global(svg) {
    flex: none;
    margin-top: 2px;
  }
  li.missing :global(svg) {
    color: var(--bad);
  }
  li.fragile :global(svg) {
    color: var(--warn);
  }
  li.unknown :global(svg) {
    color: var(--text-3);
  }
</style>
