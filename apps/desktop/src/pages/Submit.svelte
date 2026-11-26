<script lang="ts">
  // "Submit": send one of your own mods to the gallery. The app checks it
  // like the gallery's robot, makes the .zip and opens GitHub's form in the
  // browser. spacetify itself sends nothing over the internet.
  import { onMount } from "svelte";
  import { Check, CircleAlert, FolderOpen, TriangleAlert } from "@lucide/svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
  import { api, type Candidate, type ReviewView } from "../lib/api";
  import { showError, toast, tr } from "../lib/state.svelte";
  import ThemePreview from "../lib/ThemePreview.svelte";

  let candidates = $state<Candidate[] | null>(null);
  let folder = $state<string | null>(null);
  let review = $state<ReviewView | null>(null);
  let checking = $state(false);
  let preparing = $state(false);

  // Only the mods that can be sent: new ones, and updates of a mod of the
  // gallery. Those already in it at this version (or a newer one), such as
  // the ones installed from the gallery, have nothing to send.
  const sendable = (c: Candidate) => c.inGallery === "no" || c.inGallery === "older";
  const shown = $derived((candidates ?? []).filter(sendable));
  const alreadyThere = $derived((candidates ?? []).length - shown.length);

  onMount(async () => {
    try {
      candidates = await api.submitCandidates();
      const first = candidates.find(sendable);
      if (first) choose(first.folder);
    } catch (error) {
      showError(error);
    }
  });

  async function choose(path: string) {
    folder = path;
    checking = true;
    try {
      review = await api.submitReview(path);
    } catch (error) {
      review = null;
      showError(error);
    } finally {
      checking = false;
    }
  }

  async function pickFolder() {
    const path = await open({ directory: true, title: tr("Folder of your mod", "Dossier de ton mod") });
    if (typeof path === "string") choose(path);
  }

  const kindLabel = (kind: string) => (kind === "theme" ? tr("Theme", "Thème") : "Extension");

  const galleryNote = (c: Candidate | ReviewView) => {
    switch (c.inGallery) {
      case "older":
        return tr("an update of the gallery's version", "une mise à jour de la version de la galerie");
      case "same":
        return tr("already in the gallery at this version", "déjà dans la galerie à cette version");
      case "newer":
        return tr("the gallery has a newer version", "la galerie a une version plus récente");
      default:
        return tr("new in the gallery", "nouveau dans la galerie");
    }
  };

  const ready = $derived(
    !!review && review.blocking === 0 && (review.inGallery === "no" || review.inGallery === "older"),
  );

  async function prepare() {
    if (!folder || !ready) return;
    preparing = true;
    try {
      const prepared = await api.submitPrepare(folder);
      await revealItemInDir(prepared.zip).catch(() => {});
      await openUrl(prepared.formUrl);
      toast("good", tr("The .zip is ready, the form is open", "Le .zip est prêt, le formulaire est ouvert"), {
        detail: tr("Drop the .zip into the form, then send it", "Glisse le .zip dans le formulaire, puis envoie-le"),
      });
    } catch (error) {
      showError(error);
    } finally {
      preparing = false;
    }
  }

  const size = (bytes: number) =>
    bytes < 1024 * 1024
      ? `${Math.max(1, Math.round(bytes / 1024))} ${tr("KB", "Ko")}`
      : `${(bytes / 1024 / 1024).toFixed(1)} ${tr("MB", "Mo")}`;
</script>

<div class="page">
  <section class="page-head">
    <div>
      <h1>{tr("Submit a mod", "Proposer un mod")}</h1>
      <p>
        {tr(
          "Send one of your themes or extensions to the gallery. The spacetify team looks at it, then accepts or refuses it.",
          "Envoie un de tes thèmes ou une de tes extensions pour la galerie. L'équipe spacetify le regarde, puis l'accepte ou le refuse.",
        )}
      </p>
    </div>
  </section>

  <div class="grid">
    <section class="card">
      <div class="card-head"><h2>{tr("Which mod?", "Quel mod ?")}</h2></div>
      <div class="list">
        {#if candidates && candidates.length === 0}
          <p class="sub empty">{tr("No mod installed yet.", "Aucun mod installé pour l'instant.")}</p>
        {:else if candidates && shown.length === 0}
          <p class="sub empty">
            {tr(
              "Your installed mods are all in the gallery already. To send one of your own, pick its folder.",
              "Tes mods installés sont tous déjà dans la galerie. Pour envoyer un des tiens, choisis son dossier.",
            )}
          </p>
        {/if}
        {#each shown as c (c.folder)}
          <label class="choice" class:on={folder === c.folder}>
            <input type="radio" name="mod" checked={folder === c.folder} onchange={() => choose(c.folder)} />
            {#if c.palette}
              <span class="thumb"><ThemePreview palette={c.palette} height={30} /></span>
            {:else}
              <span class="thumb js mono">JS</span>
            {/if}
            <span class="text">
              <span class="name">{c.name}</span>
              <span class="meta">{kindLabel(c.kind)} · {c.version} · {galleryNote(c)}</span>
            </span>
          </label>
        {/each}
        {#if folder && !shown.some((c) => c.folder === folder)}
          <p class="meta mono picked">{folder}</p>
        {/if}
        {#if alreadyThere > 0 && shown.length > 0}
          <p class="meta">
            {tr(
              `${alreadyThere} installed mod(s) already in the gallery are not listed.`,
              `${alreadyThere} mod(s) installé(s) déjà dans la galerie ne sont pas affichés.`,
            )}
          </p>
        {/if}
        <button class="btn ghost" onclick={pickFolder}><FolderOpen size={14} strokeWidth={1.5} />{tr("Another folder…", "Un autre dossier…")}</button>
      </div>
    </section>

    <section class="card">
      <div class="card-head">
        <h2>{tr("Checked on your computer", "Vérifié sur ton ordinateur")}</h2>
        {#if review}<span class="meta">{size(review.size)}</span>{/if}
      </div>
      <div class="checks" aria-busy={checking}>
        {#if checking}
          <p class="sub">{tr("Checking…", "Vérification…")}</p>
        {:else if review}
          {#each review.checks as check (check.title)}
            <div class="check">
              <span class="icon {check.level}">
                {#if check.level === "ok"}<Check size={15} strokeWidth={2.2} />
                {:else if check.level === "look"}<TriangleAlert size={15} strokeWidth={1.8} />
                {:else}<CircleAlert size={15} strokeWidth={1.8} />{/if}
              </span>
              <div class="body">
                <p>{check.title}{#if check.clean}<span class="meta">{" · "}{check.clean}</span>{/if}</p>
                {#each check.findings as f, i (i)}
                  <p class="finding">
                    {f.what}{#if f.place}<span class="meta">{` (${f.place})`}</span>{/if}
                  </p>
                  {#if f.excerpt}<p class="excerpt mono">{f.excerpt}</p>{/if}
                {/each}
              </div>
            </div>
          {/each}
          <p class="meta">
            {tr(
              "The same checks run again on GitHub, with an antivirus.",
              "Les mêmes vérifications sont refaites sur GitHub, avec un antivirus.",
            )}
          </p>
        {:else}
          <p class="sub">{tr("Choose a mod.", "Choisis un mod.")}</p>
        {/if}
      </div>
    </section>
  </div>

  <section class="card">
    <div class="card-head"><h2>{tr("What happens next", "Ce qui va se passer")}</h2></div>
    <div class="steps">
      <ol>
        <li>
          {tr(
            "spacetify puts your mod in a .zip, in your Downloads folder.",
            "spacetify range ton mod dans un .zip, dans tes Téléchargements.",
          )}
        </li>
        <li>
          {tr(
            "Your browser opens the request form on GitHub, already filled in.",
            "Ton navigateur ouvre le formulaire de demande sur GitHub, déjà rempli.",
          )}
        </li>
        <li>{tr("Drop the .zip into it and send.", "Tu y glisses le .zip et tu envoies.")}</li>
      </ol>
      <div class="go">
        <button class="btn primary" disabled={!ready || preparing} onclick={prepare}
          >{preparing ? tr("Preparing…", "Préparation…") : tr("Prepare the request", "Préparer l'envoi")}</button
        >
        <p class="meta">
          {#if review && review.blocking > 0}
            {tr("Fix what is blocking first.", "Corrige d'abord ce qui bloque.")}
          {:else if review && (review.inGallery === "same" || review.inGallery === "newer")}
            {tr(
              "Raise the version in the manifest to send an update.",
              "Augmente la version dans le manifeste pour envoyer une mise à jour.",
            )}
          {:else}
            {tr(
              "You need a GitHub account (free, two minutes). It is how you will know whether your mod is accepted.",
              "Il faut un compte GitHub (gratuit, deux minutes). C'est par lui que tu sauras si ton mod est accepté.",
            )}
            <button class="link" onclick={() => openUrl("https://github.com/signup").catch(showError)}
              >{tr("Create an account", "Créer un compte")}</button
            >
          {/if}
        </p>
      </div>
    </div>
  </section>
</div>

<style>
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
    gap: 16px;
    align-items: start;
  }
  .list,
  .checks {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 16px 16px;
  }
  .list .btn {
    align-self: flex-start;
  }
  .choice {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    border-radius: var(--r-control);
    border: 1px solid var(--line);
    cursor: pointer;
  }
  .choice.on {
    border-color: var(--accent);
    background: var(--raised);
  }
  .choice input {
    accent-color: var(--accent);
  }
  .thumb {
    width: 44px;
    flex: none;
    border-radius: 6px;
    overflow: hidden;
  }
  .thumb.js {
    height: 30px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--raised);
    color: var(--text-2);
    font-size: 11px;
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .name {
    font-weight: 500;
  }
  .picked {
    overflow-wrap: anywhere;
  }
  .empty {
    padding: 4px 0;
  }
  .check {
    display: flex;
    gap: 10px;
    align-items: flex-start;
  }
  .icon {
    flex: none;
    margin-top: 2px;
    display: flex;
  }
  .icon.ok {
    color: var(--good);
  }
  .icon.look {
    color: var(--warn);
  }
  .icon.block {
    color: var(--bad);
  }
  .body {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }
  .finding {
    color: var(--text-2);
  }
  .excerpt {
    padding: 6px 8px;
    border-radius: 6px;
    background: var(--bg);
    color: var(--text-2);
    font-size: 12px;
    overflow-wrap: anywhere;
  }
  .steps {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 16px;
  }
  ol {
    margin: 0;
    padding-left: 20px;
    display: grid;
    gap: 6px;
    color: var(--text-2);
  }
  .go {
    display: flex;
    align-items: center;
    gap: 16px;
    flex-wrap: wrap;
  }
  .go p {
    flex: 1;
    min-width: 240px;
  }
  .link {
    border: 0;
    padding: 0;
    background: none;
    color: var(--accent);
    font: inherit;
    cursor: pointer;
    text-decoration: underline;
  }
</style>
