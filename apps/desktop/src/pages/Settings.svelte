<script lang="ts">
  import { onMount } from "svelte";
  import { ExternalLink, FolderOpen } from "@lucide/svelte";
  import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
  import { api, type SettingsView } from "../lib/api";
  import Logo from "../lib/Logo.svelte";
  import { app, refreshOverview, showError, tr } from "../lib/state.svelte";

  const REPO = "https://github.com/spacetify-mod/spacetify";
  let settings = $state<SettingsView | null>(null);

  async function load() {
    try {
      settings = await api.getSettings();
    } catch (error) {
      showError(error);
    }
  }

  onMount(load);

  async function setLanguage(language: string) {
    try {
      await api.setLanguage(language);
      await load();
      if (settings) {
        app.lang = settings.lang;
        document.documentElement.lang = settings.lang;
      }
      refreshOverview();
    } catch (error) {
      showError(error);
    }
  }

  const languages = [
    { value: "system", label: () => tr("System", "Système") },
    { value: "fr", label: () => "Français" },
    { value: "en", label: () => "English" },
  ];

  const links = [
    { label: () => "GitHub", url: REPO },
    { label: () => tr("Report a problem", "Signaler un problème"), url: `${REPO}/issues` },
    { label: () => tr("Making a theme", "Créer un thème"), url: `${REPO}/blob/main/docs/themes.md` },
    { label: () => tr("Making an extension", "Créer une extension"), url: `${REPO}/blob/main/docs/extensions.md` },
  ];
</script>

<div class="page">
  <section class="page-head">
    <h1>{tr("Settings", "Réglages")}</h1>
  </section>

  {#if settings}
    <section class="card">
      <div class="rows">
        <div class="row">
          <div class="label">
            <p>{tr("Language", "Langue")}</p>
            <p class="meta">{tr("App, notifications and the spacetify command", "Appli, notifications et commande spacetify")}</p>
          </div>
          <div class="segmented" role="radiogroup" aria-label={tr("Language", "Langue")}>
            {#each languages as option (option.value)}
              <button
                role="radio"
                aria-checked={settings.language === option.value}
                class:on={settings.language === option.value}
                onclick={() => setLanguage(option.value)}>{option.label()}</button
              >
            {/each}
          </div>
        </div>
        <div class="row">
          <div class="label">
            <p>{tr("Mods and settings", "Mods et réglages")}</p>
            <p class="meta mono">{settings.configDir}</p>
          </div>
          <button class="btn" onclick={() => revealItemInDir(settings!.configDir).catch(showError)}
            ><FolderOpen size={14} strokeWidth={1.5} />{tr("Open", "Ouvrir")}</button
          >
        </div>
        <div class="row">
          <div class="label">
            <p>{tr("Backups and service", "Sauvegardes et service")}</p>
            <p class="meta mono">{settings.dataDir}</p>
          </div>
          <button class="btn" onclick={() => revealItemInDir(settings!.dataDir).catch(showError)}
            ><FolderOpen size={14} strokeWidth={1.5} />{tr("Open", "Ouvrir")}</button
          >
        </div>
      </div>
    </section>

    <section class="card about">
      <div class="about-head">
        <Logo size={40} />
        <div>
          <p class="name">spacetify <span class="meta">{settings.version}</span></p>
          <p class="sub">{tr("Themes and extensions for Spotify on desktop", "Thèmes et extensions pour Spotify sur ordinateur")}</p>
        </div>
      </div>
      <div class="links">
        {#each links as link (link.url)}
          <button class="btn" onclick={() => openUrl(link.url).catch(showError)}
            ><ExternalLink size={14} strokeWidth={1.5} />{link.label()}</button
          >
        {/each}
      </div>
      <p class="meta">
        {tr(
          "GPL-3.0-or-later. Not affiliated with Spotify AB. Modifying the Spotify client may go against its terms of use.",
          "GPL-3.0-or-later. Sans lien avec Spotify AB. Modifier le client Spotify peut aller contre ses conditions d'utilisation.",
        )}
      </p>
    </section>
  {/if}
</div>

<style>
  .segmented {
    display: inline-flex;
    padding: 2px;
    gap: 2px;
    border-radius: var(--r-control);
    background: var(--bg);
    border: 1px solid var(--line);
  }
  .segmented button {
    height: 28px;
    padding: 0 12px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--text-2);
    font-size: 13px;
    cursor: pointer;
    transition:
      background-color 150ms var(--ease),
      color 150ms var(--ease);
  }
  .segmented button.on {
    background: var(--raised);
    color: var(--text);
  }
  .label .mono {
    word-break: break-all;
  }
  .about {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 16px;
  }
  .about-head {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .name {
    font-weight: 500;
  }
  .links {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
</style>
