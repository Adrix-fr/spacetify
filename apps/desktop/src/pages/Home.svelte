<script lang="ts">
  import { onMount } from "svelte";
  import { Play, RotateCcw, TriangleAlert, CircleAlert, Info } from "@lucide/svelte";
  import { api, type DoctorView, type EventView } from "../lib/api";
  import { app, runAction, showError, tr } from "../lib/state.svelte";
  import ThemePreview from "../lib/ThemePreview.svelte";

  let doctor = $state<DoctorView | null>(null);
  let doctorFailed = $state(false);
  let recent = $state<EventView[]>([]);

  const overview = $derived(app.overview);
  const spotify = $derived(overview?.spotify);
  const service = $derived(overview?.service);

  const status = $derived.by(() => {
    if (!spotify) return null;
    if (!spotify.found) return { dot: "bad", text: tr("Not found", "Introuvable") };
    if (spotify.readOnly) return { dot: "bad", text: tr("Read-only install", "Installation en lecture seule") };
    switch (spotify.state) {
      case "patched":
        return { dot: "on", text: tr("Mods applied", "Mods appliqués") };
      case "spicetify":
        return { dot: "warn", text: tr("Modified by Spicetify", "Modifié par Spicetify") };
      case "original":
        return spotify.updateReplaced
          ? { dot: "warn", text: tr("Mods removed by an update", "Mods retirés par une mise à jour") }
          : { dot: "", text: tr("Original", "D'origine") };
      default:
        return { dot: "bad", text: spotify.stateLabel ?? "" };
    }
  });

  const canChange = $derived(!!spotify?.found && !spotify.readOnly && spotify.state !== "spicetify" && !app.busy);

  const compat = $derived.by(() => {
    if (doctorFailed) return { dot: "bad", text: tr("Could not check", "Vérification impossible") };
    if (!doctor) return { dot: "", text: "…" };
    switch (doctor.verdict) {
      case "nothing":
        return { dot: "", text: tr("Nothing to check", "Rien à vérifier") };
      case "compatible":
        return { dot: "good", text: tr("Compatible", "Compatible") };
      case "warnings":
        return { dot: "warn", text: tr("Warnings", "Avertissements") };
      default:
        return { dot: "bad", text: tr("Partly broken", "En partie cassé") };
    }
  });

  async function loadExtras() {
    try {
      recent = (await api.repairInfo(4)).events;
    } catch {
      recent = [];
    }
    try {
      doctor = await api.doctor();
      doctorFailed = false;
    } catch {
      doctor = null;
      doctorFailed = true;
    }
  }

  onMount(() => {
    loadExtras();
  });

  async function act(kind: "apply" | "restore") {
    await runAction(kind);
    loadExtras();
  }

  async function openSpotify() {
    try {
      await api.launchSpotify();
    } catch (error) {
      showError(error);
    }
  }
</script>

{#if !overview || !spotify || !service || !status}
  <p class="sub">{tr("Looking for Spotify…", "Recherche de Spotify…")}</p>
{:else}
  <div class="page">
    <section class="page-head">
      <div>
        <h1>{spotify.found ? `Spotify ${spotify.version ?? ""}` : tr("Spotify not found", "Spotify introuvable")}</h1>
        <p class="status">
          <span class="dot {status.dot}"></span>
          {status.text}
          {#if spotify.found}
            <span class="meta">·</span>
            {spotify.running ? tr("open", "ouvert") : tr("closed", "fermé")}
          {/if}
        </p>
      </div>
      <div class="actions">
        {#if spotify.found && !spotify.running}
          <button class="btn ghost" onclick={openSpotify}>{tr("Open Spotify", "Ouvrir Spotify")}</button>
        {/if}
        <button class="btn" disabled={!canChange} onclick={() => act("restore")}>
          <RotateCcw size={14} strokeWidth={1.5} />{tr("Restore", "Restaurer")}
        </button>
        <button class="btn primary" disabled={!canChange} onclick={() => act("apply")}>
          <Play size={14} strokeWidth={1.5} />{tr("Apply", "Appliquer")}
        </button>
      </div>
    </section>

    {#if spotify.error}
      <div class="notice bad"><CircleAlert size={16} strokeWidth={1.5} /><span>{spotify.error}</span></div>
    {/if}
    {#if spotify.state === "spicetify"}
      <div class="notice warn">
        <TriangleAlert size={16} strokeWidth={1.5} />
        <span
          >{tr(
            "Spicetify has modified Spotify, so spacetify changes nothing. To switch, run `spicetify restore` in a terminal, then Apply.",
            "Spicetify a modifié Spotify, donc spacetify ne touche à rien. Pour changer, lance `spicetify restore` dans un terminal, puis Appliquer.",
          )}</span
        >
      </div>
    {/if}
    {#if spotify.updateReplaced}
      <div class="notice warn">
        <TriangleAlert size={16} strokeWidth={1.5} />
        <span
          >{service.running
            ? tr(
                "A Spotify update removed the mods. The service puts them back when Spotify closes.",
                "Une mise à jour de Spotify a retiré les mods. Le service les remet à la fermeture de Spotify.",
              )
            : tr(
                "A Spotify update removed the mods. Apply puts them back.",
                "Une mise à jour de Spotify a retiré les mods. Appliquer les remet.",
              )}</span
        >
      </div>
    {/if}
    {#if overview.modsError}
      <div class="notice bad"><CircleAlert size={16} strokeWidth={1.5} /><span>{overview.modsError}</span></div>
    {/if}
    {#if spotify.storeAlsoInstalled}
      <div class="notice">
        <Info size={16} strokeWidth={1.5} />
        <span
          >{tr(
            "The Microsoft Store version is also installed. Only the regular one is modified.",
            "La version Microsoft Store est aussi installée. Seule la version classique est modifiée.",
          )}</span
        >
      </div>
    {/if}

    <section class="grid">
      <article class="card mods">
        <ThemePreview palette={overview.theme?.palette ?? spotifyLook} height={200} />
        <div class="mods-foot">
          <div>
            <p class="name">{overview.theme?.name ?? tr("Spotify's colours", "Couleurs d'origine")}</p>
            <p class="meta">
              {overview.extensions.length === 0
                ? tr("No extension", "Aucune extension")
                : overview.extensions.join(", ")}
            </p>
          </div>
          <button class="btn" onclick={() => (app.page = "themes")}>{tr("Change", "Changer")}</button>
        </div>
      </article>

      <article class="card">
        <div class="rows">
          <button class="row link" onclick={() => (app.page = "repair")}>
            <span class="label">{tr("Automatic repair", "Réparation auto")}</span>
            <span class="value"><span class="dot {service.running ? 'on' : ''}"></span>{service.running ? tr("On", "Active") : tr("Off", "Arrêtée")}</span>
          </button>
          <button class="row link" onclick={() => (app.page = "repair")}>
            <span class="label">{tr("Starts with Windows", "Démarre avec Windows")}</span>
            <span class="value">{service.autostart === "on" ? tr("Yes", "Oui") : tr("No", "Non")}</span>
          </button>
          <button class="row link" onclick={() => (app.page = "compat")}>
            <span class="label">{tr("Compatibility", "Compatibilité")}</span>
            <span class="value"><span class="dot {compat.dot}"></span>{compat.text}</span>
          </button>
          <div class="row">
            <span class="label">{tr("Last repair", "Dernière réparation")}</span>
            <span class="value">{service.lastRepair ?? tr("Never", "Jamais")}</span>
          </div>
          <div class="row">
            <span class="label">{tr("Backups", "Sauvegardes")}</span>
            <span class="value">{spotify.backups}</span>
          </div>
        </div>
      </article>
    </section>

    <section class="card">
      <div class="card-head">
        <h2>{tr("Recent activity", "Activité récente")}</h2>
        <button class="btn ghost" onclick={() => (app.page = "repair")}>{tr("See all", "Tout voir")}</button>
      </div>
      {#if recent.length === 0}
        <p class="empty sub">{tr("Nothing yet", "Rien pour l'instant")}</p>
      {:else}
        <div class="rows">
          {#each recent as event (event.at + event.text)}
            <div class="row">
              <span class="dot {event.tone}"></span>
              <span class="label">{event.text}</span>
              <span class="meta">{event.when}</span>
            </div>
          {/each}
        </div>
      {/if}
    </section>
  </div>
{/if}

<script module lang="ts">
  // Spotify's own look, for the preview when no theme is on.
  const spotifyLook = {
    frame: "#000000",
    background: "#121212",
    highlight: "#1f1f1f",
    elevated: "#2a2a2a",
    text: "#b3b3b3",
    accent: "#1ed760",
    swatches: [],
  };
</script>

<style>
  .status {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .actions {
    display: flex;
    gap: 8px;
  }
  .grid {
    display: grid;
    grid-template-columns: minmax(0, 3fr) minmax(0, 2fr);
    gap: 16px;
    align-items: start;
  }
  .mods {
    overflow: hidden;
  }
  .mods :global(.preview) {
    border-bottom: 1px solid var(--line);
  }
  .mods-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 12px 16px;
  }
  .name {
    font-weight: 500;
  }
  .row.link {
    width: 100%;
    border: 0;
    background: transparent;
    text-align: left;
    cursor: pointer;
    transition: background-color 150ms var(--ease);
  }
  .row.link:hover {
    background: var(--raised);
  }
  .rows .row:first-child.link {
    border-radius: 11px 11px 0 0;
  }
  .value {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }
  .empty {
    padding: 12px 16px;
  }
  @media (max-width: 860px) {
    .grid {
      grid-template-columns: 1fr;
    }
    .page-head {
      flex-direction: column;
      align-items: flex-start;
    }
  }
</style>
