<script lang="ts">
  import { onMount } from "svelte";
  import { FolderPlus, SlidersHorizontal, Trash2, CircleAlert } from "@lucide/svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { api, type ModsView, type ModView, type SettingView } from "../lib/api";
  import { ask, changedToast, refreshOverview, showError, tr } from "../lib/state.svelte";
  import Toggle from "../lib/Toggle.svelte";

  let mods = $state<ModsView | null>(null);
  let opened = $state<Record<string, boolean>>({});

  async function load() {
    try {
      mods = await api.listMods();
    } catch (error) {
      showError(error);
    }
  }

  onMount(load);

  const examples = $derived(mods?.examples.filter((e) => e.kind === "extension" && !e.installed) ?? []);
  const later = tr("Shows in Spotify after Apply", "Visible dans Spotify après Appliquer");

  async function after(job: () => Promise<unknown>) {
    try {
      await job();
    } catch (error) {
      showError(error);
    }
    await load();
    refreshOverview();
  }

  const toggle = (ext: ModView, on: boolean) =>
    after(async () => {
      await api.setExtension(ext.id, on);
      changedToast(on ? tr(`${ext.name} on`, `${ext.name} activée`) : tr(`${ext.name} off`, `${ext.name} désactivée`), later);
    });

  const save = (ext: ModView, setting: SettingView, value: unknown | null) =>
    after(async () => {
      await api.setSetting(ext.id, setting.name, value);
      changedToast(tr(`${setting.label} saved`, `${setting.label} enregistré`), later);
    });

  const install = (job: () => Promise<{ message: string; detail: string | null }>) =>
    after(async () => {
      const done = await job();
      changedToast(done.message, done.detail);
    });

  async function pickFolder() {
    const path = await open({ directory: true, title: tr("Extension folder", "Dossier de l'extension") });
    if (typeof path === "string") install(() => api.installMod(path));
  }

  async function uninstall(ext: ModView) {
    const ok = await ask({
      title: tr(`Uninstall ${ext.name}?`, `Désinstaller ${ext.name} ?`),
      body: tr(
        "Its folder and settings are deleted. Spotify keeps it until the next Apply.",
        "Son dossier et ses réglages sont supprimés. Spotify la garde jusqu'au prochain Appliquer.",
      ),
      confirm: tr("Uninstall", "Désinstaller"),
    });
    if (ok) install(() => api.uninstallMod(ext.id));
  }

  function numberFrom(input: string): number | null {
    const value = Number(input.replace(",", "."));
    return input.trim() !== "" && Number.isFinite(value) ? value : null;
  }
</script>

<div class="page">
  <section class="page-head">
    <div>
      <h1>Extensions</h1>
      <p>{tr("Small additions to Spotify", "Petits ajouts à Spotify")}</p>
    </div>
    <button class="btn" onclick={pickFolder}><FolderPlus size={14} strokeWidth={1.5} />{tr("Install a folder", "Installer un dossier")}</button>
  </section>

  {#if mods}
    <section class="card">
      {#if mods.extensions.length === 0}
        <p class="empty sub">{tr("No extension installed", "Aucune extension installée")}</p>
      {/if}
      {#each mods.extensions as ext (ext.id)}
        <article class="ext">
          <div class="head">
            <div class="text">
              <p><span class="name">{ext.name}</span> <span class="meta">{ext.version ?? ""}</span></p>
              {#if ext.error}
                <p class="error"><CircleAlert size={14} strokeWidth={1.5} />{ext.error}</p>
              {:else}
                <p class="sub">{ext.description ?? ""}</p>
                <p class="meta">{ext.author}</p>
              {/if}
            </div>
            {#if ext.settings.length > 0}
              <button
                class="btn ghost"
                class:open={opened[ext.id]}
                aria-expanded={!!opened[ext.id]}
                onclick={() => (opened[ext.id] = !opened[ext.id])}
              >
                <SlidersHorizontal size={14} strokeWidth={1.5} />{tr("Settings", "Réglages")}
              </button>
            {/if}
            <button
              class="btn ghost icon"
              aria-label={tr(`Uninstall ${ext.name}`, `Désinstaller ${ext.name}`)}
              title={tr("Uninstall", "Désinstaller")}
              onclick={() => uninstall(ext)}><Trash2 size={14} strokeWidth={1.5} /></button
            >
            <Toggle
              checked={ext.active}
              disabled={!!ext.error}
              label={tr(`${ext.name} on or off`, `${ext.name} activée ou non`)}
              onchange={(on) => toggle(ext, on)}
            />
          </div>
          {#if opened[ext.id]}
            <div class="settings">
              {#each ext.settings as setting (setting.name)}
                <div class="setting">
                  <label for="{ext.id}-{setting.name}">{setting.label}</label>
                  <div class="control">
                    {#if setting.kind === "boolean"}
                      <Toggle checked={setting.value === true} label={setting.label} onchange={(v) => save(ext, setting, v)} />
                    {:else if setting.kind === "choice"}
                      <select
                        id="{ext.id}-{setting.name}"
                        value={String(setting.value)}
                        onchange={(e) => save(ext, setting, e.currentTarget.value)}
                      >
                        {#each setting.choices as choice (choice)}
                          <option value={choice}>{choice}</option>
                        {/each}
                      </select>
                    {:else if setting.kind === "number"}
                      <input
                        id="{ext.id}-{setting.name}"
                        type="number"
                        value={String(setting.value)}
                        onchange={(e) => {
                          const value = numberFrom(e.currentTarget.value);
                          if (value !== null) save(ext, setting, value);
                        }}
                      />
                    {:else}
                      <input
                        id="{ext.id}-{setting.name}"
                        type="text"
                        value={String(setting.value ?? "")}
                        onchange={(e) => save(ext, setting, e.currentTarget.value)}
                      />
                    {/if}
                    {#if setting.custom}
                      <button class="btn ghost" onclick={() => save(ext, setting, null)}>{tr("Reset", "Par défaut")}</button>
                    {/if}
                  </div>
                </div>
              {/each}
            </div>
          {/if}
        </article>
      {/each}
    </section>

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
  .ext + .ext {
    border-top: 1px solid var(--line);
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 16px;
  }
  .text {
    flex: 1;
    min-width: 0;
  }
  .name {
    font-weight: 500;
  }
  .error {
    display: flex;
    gap: 8px;
    align-items: center;
    color: var(--bad);
  }
  .btn.open {
    background: var(--raised);
    color: var(--text);
  }
  .icon {
    width: 32px;
    padding: 0;
  }
  .settings {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 0 16px 16px;
    padding: 12px 16px;
    border-radius: 8px;
    background: var(--bg);
    border: 1px solid var(--line);
  }
  .setting {
    display: grid;
    grid-template-columns: minmax(120px, 220px) 1fr;
    gap: 16px;
    align-items: center;
    min-height: 32px;
  }
  .setting label {
    color: var(--text-2);
  }
  .control {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .control input,
  .control select {
    width: min(280px, 100%);
  }
  .empty {
    padding: 16px;
  }
</style>
