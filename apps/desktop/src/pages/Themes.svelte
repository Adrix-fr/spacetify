<script lang="ts">
  import { onMount } from "svelte";
  import { FolderPlus, Trash2, CircleAlert } from "@lucide/svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { api, type ModsView } from "../lib/api";
  import { ask, changedToast, refreshOverview, showError, tr } from "../lib/state.svelte";
  import ThemePreview from "../lib/ThemePreview.svelte";

  let mods = $state<ModsView | null>(null);

  async function load() {
    try {
      mods = await api.listMods();
    } catch (error) {
      showError(error);
    }
  }

  onMount(load);

  const active = $derived(mods?.themes.find((t) => t.active) ?? null);
  const examples = $derived(mods?.examples.filter((e) => e.kind === "theme" && !e.installed) ?? []);

  async function after(job: () => Promise<unknown>) {
    try {
      await job();
    } catch (error) {
      showError(error);
    }
    await load();
    refreshOverview();
  }

  const choose = (id: string | null, name: string) =>
    after(async () => {
      await api.setTheme(id);
      changedToast(
        id ? tr(`Theme: ${name}`, `Thème : ${name}`) : tr("Theme: Spotify's colours", "Thème : couleurs d'origine"),
        tr("Shows in Spotify after Apply", "Visible dans Spotify après Appliquer"),
      );
    });

  const install = (job: () => Promise<{ message: string; detail: string | null }>) =>
    after(async () => {
      const done = await job();
      changedToast(done.message, done.detail);
    });

  async function pickFolder() {
    const path = await open({ directory: true, title: tr("Theme folder", "Dossier du thème") });
    if (typeof path === "string") install(() => api.installMod(path));
  }

  async function uninstall(id: string, name: string) {
    const ok = await ask({
      title: tr(`Uninstall ${name}?`, `Désinstaller ${name} ?`),
      body: tr(
        "Its folder is deleted. Spotify keeps it until the next Apply.",
        "Son dossier est supprimé. Spotify le garde jusqu'au prochain Appliquer.",
      ),
      confirm: tr("Uninstall", "Désinstaller"),
    });
    if (ok) install(() => api.uninstallMod(id));
  }
</script>

<div class="page">
  <section class="page-head">
    <div>
      <h1>{tr("Themes", "Thèmes")}</h1>
      <p>{tr("One theme at a time", "Un thème à la fois")}</p>
    </div>
    <button class="btn" onclick={pickFolder}><FolderPlus size={14} strokeWidth={1.5} />{tr("Install a folder", "Installer un dossier")}</button>
  </section>

  {#if mods}
    <section class="grid">
      <article class="card theme" class:active={!active}>
        <ThemePreview palette={mods.original} height={128} />
        <div class="foot">
          <div class="text">
            <p class="name">{tr("Spotify's colours", "Couleurs d'origine")}</p>
            <p class="meta">{tr("No theme", "Aucun thème")}</p>
          </div>
          {#if !active}
            <span class="in-use"><span class="dot on"></span>{tr("In use", "Actif")}</span>
          {:else}
            <button class="btn" onclick={() => choose(null, "")}>{tr("Use", "Utiliser")}</button>
          {/if}
        </div>
      </article>

      {#each mods.themes as theme (theme.id)}
        <article class="card theme" class:active={theme.active}>
          {#if theme.palette}
            <ThemePreview palette={theme.palette} height={128} />
          {:else}
            <div class="broken"><CircleAlert size={18} strokeWidth={1.5} /></div>
          {/if}
          <div class="foot">
            <div class="text">
              <p class="name">{theme.name}</p>
              {#if theme.error}
                <p class="meta error">{theme.error}</p>
              {:else}
                <p class="meta">{theme.author} · {theme.version}</p>
              {/if}
            </div>
            {#if theme.active}
              <span class="in-use"><span class="dot on"></span>{tr("In use", "Actif")}</span>
            {:else if !theme.error}
              <button class="btn" onclick={() => choose(theme.id, theme.name)}>{tr("Use", "Utiliser")}</button>
            {/if}
            <button
              class="btn ghost icon"
              aria-label={tr(`Uninstall ${theme.name}`, `Désinstaller ${theme.name}`)}
              title={tr("Uninstall", "Désinstaller")}
              onclick={() => uninstall(theme.id, theme.name)}><Trash2 size={14} strokeWidth={1.5} /></button
            >
          </div>
        </article>
      {/each}
    </section>

    {#if active?.palette && active.palette.swatches.length > 0}
      <section class="card">
        <div class="card-head"><h2>{tr(`${active.name} colours`, `Couleurs de ${active.name}`)}</h2></div>
        <div class="swatches">
          {#each active.palette.swatches.slice(0, 16) as swatch (swatch.label + swatch.color)}
            <div class="swatch">
              <span class="chip" style:background={swatch.color}></span>
              <span class="sw-label">{swatch.label}</span>
              <span class="mono meta">{swatch.color}</span>
            </div>
          {/each}
        </div>
      </section>
    {/if}

    {#if examples.length > 0}
      <section class="card">
        <div class="card-head"><h2>{tr("Included examples", "Exemples fournis")}</h2></div>
        <div class="rows">
          {#each examples as example (example.id)}
            <div class="row">
              <div class="label">
                <p>{example.name}</p>
                {#if example.description}<p class="meta">{example.description}</p>{/if}
              </div>
              <button class="btn" onclick={() => install(() => api.installExample(example.id))}
                >{tr("Install", "Installer")}</button
              >
            </div>
          {/each}
        </div>
      </section>
    {/if}

    <p class="meta">{tr("Folder:", "Dossier :")}&nbsp;<span class="mono">{mods.folder}</span></p>
  {/if}
</div>

<style>
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: 16px;
  }
  .theme {
    overflow: hidden;
    transition: border-color 150ms var(--ease);
  }
  .theme.active {
    border-color: rgba(255, 107, 129, 0.55);
  }
  .theme :global(.preview) {
    border-bottom: 1px solid var(--line);
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 12px 12px 16px;
  }
  .text {
    flex: 1;
    min-width: 0;
  }
  .name {
    font-weight: 500;
  }
  .error {
    color: var(--bad);
  }
  .in-use {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--text-2);
    padding: 0 8px;
  }
  .icon {
    width: 32px;
    padding: 0;
  }
  .broken {
    height: 128px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--bad);
    border-bottom: 1px solid var(--line);
  }
  .swatches {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 4px 16px;
    padding: 12px 16px;
  }
  .swatch {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 32px;
  }
  .chip {
    width: 20px;
    height: 20px;
    border-radius: 6px;
    border: 1px solid var(--line-strong);
    flex: none;
  }
  .sw-label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
