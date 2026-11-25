<script lang="ts">
  import { onMount } from "svelte";
  import { RefreshCw, CircleAlert, FileText } from "@lucide/svelte";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { api, type RepairView } from "../lib/api";
  import { refreshOverview, showError, toast, tr, withBusy } from "../lib/state.svelte";
  import Toggle from "../lib/Toggle.svelte";

  let info = $state<RepairView | null>(null);

  async function load() {
    try {
      info = await api.repairInfo(40);
    } catch (error) {
      showError(error);
    }
  }

  onMount(() => {
    load();
    const timer = setInterval(() => {
      if (!document.hidden) load();
    }, 3000);
    return () => clearInterval(timer);
  });

  async function setRunning(on: boolean) {
    try {
      const done = await withBusy(
        on ? tr("Starting the service…", "Démarrage du service…") : tr("Stopping the service…", "Arrêt du service…"),
        () => api.setServiceRunning(on),
      );
      toast("good", done.message, { detail: done.detail });
    } catch (error) {
      showError(error);
    }
    await load();
    refreshOverview();
  }

  async function setAutostart(on: boolean) {
    try {
      const done = await api.setAutostart(on);
      toast("good", done.message, { detail: done.detail });
    } catch (error) {
      showError(error);
    }
    await load();
    refreshOverview();
  }

  async function check() {
    try {
      await api.serviceCheck();
      toast("info", tr("Checking Spotify now", "Vérification de Spotify en cours"));
    } catch (error) {
      showError(error);
    }
  }
</script>

<div class="page">
  <section class="page-head">
    <div>
      <h1>{tr("Automatic repair", "Réparation automatique")}</h1>
      <p>
        {tr(
          "Puts the mods back after a Spotify update, once Spotify is closed",
          "Remet les mods après une mise à jour de Spotify, dès qu'il est fermé",
        )}
      </p>
    </div>
    {#if info?.service.running}
      <button class="btn" onclick={check}><RefreshCw size={14} strokeWidth={1.5} />{tr("Check now", "Vérifier")}</button>
    {/if}
  </section>

  {#if info}
    {@const s = info.service}
    {#if !s.installed}
      <div class="notice warn">
        <CircleAlert size={16} strokeWidth={1.5} />
        <span
          >{tr(
            "spacetify-service was not found next to the app, so it cannot be started from here.",
            "spacetify-service est introuvable à côté de l'appli, il ne peut donc pas être lancé d'ici.",
          )}</span
        >
      </div>
    {/if}

    <section class="card">
      <div class="rows">
        <div class="row">
          <div class="label">
            <p>{tr("Service", "Service")}</p>
            <p class="meta">
              {#if s.running}
                {s.phase}{s.started ? tr(` · started ${s.started}`, ` · lancé ${s.started}`) : ""}
              {:else}
                {tr("Stopped", "Arrêté")}
              {/if}
            </p>
          </div>
          <span class="state"><span class="dot {s.running ? 'on' : ''}"></span>{s.running ? tr("Running", "En marche") : tr("Off", "Arrêté")}</span>
          <Toggle checked={s.running} disabled={!s.installed} label={tr("Service running", "Service en marche")} onchange={setRunning} />
        </div>
        <div class="row">
          <div class="label">
            <p>{tr("Start with Windows", "Démarrer avec Windows")}</p>
            <p class="meta">
              {s.autostartDetail ??
                tr("Shows as spacetify in Task Manager > Startup apps", "Visible comme spacetify dans Gestionnaire des tâches > Démarrage")}
            </p>
          </div>
          <Toggle
            checked={s.autostart === "on"}
            disabled={!s.installed}
            label={tr("Start with Windows", "Démarrer avec Windows")}
            onchange={setAutostart}
          />
        </div>
        <div class="row">
          <span class="label">{tr("Last repair", "Dernière réparation")}</span>
          <span class="value">{s.lastRepair ?? tr("Never", "Jamais")}</span>
        </div>
      </div>
    </section>

    <section class="card">
      <div class="card-head">
        <h2>{tr("History", "Historique")}</h2>
        <button class="btn ghost" onclick={() => revealItemInDir(info!.log).catch(showError)}
          ><FileText size={14} strokeWidth={1.5} />{tr("Technical log", "Journal technique")}</button
        >
      </div>
      {#if info.events.length === 0}
        <p class="empty sub">{tr("Nothing yet", "Rien pour l'instant")}</p>
      {:else}
        <div class="rows">
          {#each info.events as event (event.at + event.text)}
            <div class="row">
              <span class="dot {event.tone}"></span>
              <span class="label text">{event.text}</span>
              <span class="meta when" title={event.at}>{event.when}</span>
            </div>
          {/each}
        </div>
      {/if}
    </section>
  {/if}
</div>

<style>
  .state {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    color: var(--text-2);
    margin-right: 8px;
  }
  .text {
    white-space: pre-line;
  }
  .when {
    white-space: nowrap;
  }
  .empty {
    padding: 12px 16px;
  }
</style>
