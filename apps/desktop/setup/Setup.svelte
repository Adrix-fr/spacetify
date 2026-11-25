<script lang="ts">
  // Every screen of the installer and the way from one to the next.
  // The look follows the app ("Corail"); the work is done by the Rust side.
  import { onMount } from "svelte";
  import {
    ArrowRight,
    Check,
    HardDrive,
    Info,
    ShieldCheck,
    TriangleAlert,
  } from "@lucide/svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import Logo from "../src/lib/Logo.svelte";
  import Frame from "./Frame.svelte";
  import StepList from "./StepList.svelte";
  import {
    api,
    events,
    type Context,
    type FailedStep,
    type Failure,
    type Program,
    type Step,
    type StepState,
  } from "./api";
  import { tr, ui } from "./i18n.svelte";
  import {
    ANTIVIRUS,
    LICENSE,
    RELEASES,
    stepConsequence,
    stepLabel,
    windowTitle,
    type Op,
  } from "./texts";

  type Screen =
    | "loading"
    | "welcome"
    | "maintenance"
    | "update"
    | "newer"
    | "uninstall"
    | "running"
    | "progress"
    | "done"
    | "uninstalled"
    | "partial"
    | "error";

  let ctx = $state<Context | null>(null);
  let screen = $state<Screen>("loading");
  let op = $state<Op>({ kind: "install" });

  // Choices on the screens.
  let choice = $state<"repair" | "uninstall">("repair");
  let restore = $state(false);
  let openApp = $state(true);

  // Work in progress.
  let states = $state<Partial<Record<Step, StepState>>>({});
  let share = $state(0);
  let cancellable = $state(false);
  let askStop = $state(false);

  // What went wrong.
  let programs = $state<Program[]>([]);
  let closing = $state(false);
  let closeFailed = $state(false);
  let failed = $state<FailedStep[]>([]);
  let failure = $state<Failure | null>(null);
  let failedStep = $state<Step | null>(null);
  let retrying = $state(false);

  const uninstallMode = $derived(ctx?.mode === "uninstall");
  const title = $derived(
    windowTitle(screen === "loading" ? null : op.kind, uninstallMode || screen === "uninstall"),
  );
  const steps = $derived<Step[]>(
    op.kind === "uninstall"
      ? op.restore
        ? ["restore", "undo", "remove"]
        : ["undo", "remove"]
      : ["files", "path", "notifications", "autostart"],
  );
  const active = $derived(steps.find((step) => states[step] === "active"));
  const percent = $derived(Math.round(share * 100));
  const drive = $derived(/^[A-Za-z]:/.test(ctx?.dir ?? "") ? ctx!.dir.slice(0, 2) : "C:");

  function firstScreen(context: Context): Screen {
    if (context.mode === "uninstall") return "uninstall";
    switch (context.relation) {
      case "fresh":
        return "welcome";
      case "older":
        return "update";
      case "newer":
        return "newer";
      default:
        return "maintenance";
    }
  }

  onMount(() => {
    const root = document.documentElement;
    const keyboard = (e: KeyboardEvent) => {
      if (e.key === "Tab") root.dataset.keyboard = "";
    };
    const pointer = () => delete root.dataset.keyboard;
    window.addEventListener("keydown", keyboard);
    window.addEventListener("pointerdown", pointer);

    const unlisten = [
      events.step(({ step, state }) => {
        states[step] = state;
      }),
      events.progress((value) => {
        share = value;
      }),
      events.committed(() => {
        cancellable = false;
        askStop = false;
      }),
      events.closeRequested(() => {
        if (screen === "progress" && cancellable) askStop = true;
      }),
      events.done(async (outcome) => {
        askStop = false;
        cancellable = false;
        switch (outcome.result) {
          case "done":
            screen = op.kind === "uninstall" ? "uninstalled" : "done";
            break;
          case "partial":
            failed = outcome.failed;
            screen = "partial";
            break;
          case "cancelled":
            await api.quit();
            break;
          case "failed":
            failure = outcome.failure;
            failedStep = outcome.step;
            if (failure.kind === "inUse") {
              programs = await api.running();
              if (programs.length > 0) {
                screen = "running";
                break;
              }
            }
            screen = "error";
            break;
        }
      }),
    ];

    (async () => {
      const context = await api.context();
      ui.lang = context.lang;
      root.lang = context.lang;
      ctx = context;
      screen = firstScreen(context);
    })();

    return () => {
      window.removeEventListener("keydown", keyboard);
      window.removeEventListener("pointerdown", pointer);
      unlisten.forEach((p) => p.then((stop) => stop()));
    };
  });

  /** Starts `next`, after asking to close spacetify if it runs. */
  async function begin(next: Op) {
    op = next;
    closeFailed = false;
    programs = await api.running();
    if (programs.length > 0) {
      screen = "running";
      return;
    }
    await run();
  }

  async function run() {
    states = {};
    share = 0;
    askStop = false;
    cancellable = op.kind !== "uninstall";
    screen = "progress";
    await api.start(op.kind === "uninstall", Boolean(op.restore));
  }

  async function closeAndContinue() {
    closing = true;
    closeFailed = false;
    const closed = await api.closeRunning();
    closing = false;
    if (closed) await run();
    else closeFailed = true;
  }

  async function retryFailed() {
    retrying = true;
    const still = await api.retry(failed.map((item) => item.step));
    retrying = false;
    if (still.length === 0) screen = "done";
    else failed = still;
  }

  async function launchAndQuit() {
    try {
      await api.launchApp();
    } finally {
      await api.quit();
    }
  }

  async function finish() {
    if (openApp) {
      try {
        await api.launchApp();
      } catch {
        // The app can still be opened from the Start menu.
      }
    }
    await api.quit();
  }

  function onClose() {
    if (screen === "progress") {
      if (cancellable) askStop = true;
      return;
    }
    api.quit();
  }

  function stop() {
    askStop = false;
    api.cancel();
  }

  const programLabel = (program: Program) =>
    program === "app"
      ? {
          name: "spacetify",
          detail: tr(
            "Window or icon next to the clock",
            "Fenêtre ou icône à côté de l'horloge",
          ),
        }
      : {
          name: tr("spacetify in a terminal", "spacetify dans un terminal"),
          detail: tr("A command is running", "Une commande en cours"),
        };

  const progressTitle = () => {
    switch (op.kind) {
      case "repair":
        return tr("Repairing", "Réparation en cours");
      case "update":
        return tr("Updating", "Mise à jour en cours");
      case "uninstall":
        return tr("Uninstalling", "Désinstallation en cours");
      default:
        return tr("Installing", "Installation en cours");
    }
  };
</script>

{#snippet logo(size: number)}
  <Logo {size} />
{/snippet}

{#snippet sign(kind: "warn" | "info" | "done", Icon: typeof Info)}
  <div class="sign {kind}" aria-hidden="true"><Icon size={20} strokeWidth={1.5} /></div>
{/snippet}

<Frame
  {title}
  canClose={!(screen === "progress" && !cancellable)}
  onclose={onClose}
  bare={screen === "progress" || screen === "loading"}
>
  {#if screen === "welcome" && ctx}
    <div class="center enter">
      {@render logo(76)}
      <h1 class="name">spacetify</h1>
      <p class="lead">
        {tr(
          "Themes and extensions for Spotify, put back by themselves after each update.",
          "Des thèmes et des extensions pour Spotify, remis en place tout seuls après chaque mise à jour.",
        )}
      </p>
      <div class="pill">
        <ShieldCheck size={14} strokeWidth={1.5} color="var(--good)" />
        <span>
          {tr(
            "For your Windows account only, without admin rights",
            "Pour ton compte Windows seulement, sans droits administrateur",
          )}
        </span>
      </div>
    </div>
  {:else if screen === "maintenance" && ctx}
    <div class="pad enter">
      <div class="head">
        {@render logo(44)}
        <div>
          <h1>
            {ctx.installedVersion
              ? tr(
                  `spacetify ${ctx.installedVersion} is already installed`,
                  `spacetify ${ctx.installedVersion} est déjà installé`,
                )
              : tr("spacetify is already installed", "spacetify est déjà installé")}
          </h1>
          <p class="sub">{tr("What do you want to do?", "Que veux-tu faire ?")}</p>
        </div>
      </div>
      <div class="options">
        <label class="option" class:chosen={choice === "repair"}>
          <input type="radio" name="choice" value="repair" bind:group={choice} />
          <span>
            <strong>{tr("Repair", "Réparer")}</strong>
            <span class="detail">
              {tr(
                "Reinstalls spacetify over itself, losing nothing: your mods, settings and backups stay.",
                "Réinstalle spacetify par-dessus, sans rien perdre : tes mods, réglages et sauvegardes sont gardés.",
              )}
            </span>
          </span>
        </label>
        <label class="option" class:chosen={choice === "uninstall"}>
          <input type="radio" name="choice" value="uninstall" bind:group={choice} />
          <span>
            <strong>{tr("Uninstall", "Désinstaller")}</strong>
            <span class="detail">
              {tr(
                "Removes spacetify from this computer. You choose next whether Spotify is put back as it was.",
                "Retire spacetify de l'ordinateur. Tu choisis à l'étape suivante si Spotify est remis d'origine.",
              )}
            </span>
          </span>
        </label>
      </div>
    </div>
  {:else if screen === "update" && ctx}
    <div class="center enter">
      {@render logo(64)}
      <h1 class="big">{tr("An update is ready", "Une mise à jour est prête")}</h1>
      <div class="versions">
        <span class="faint">{ctx.installedVersion}</span>
        <ArrowRight size={14} strokeWidth={1.75} color="var(--accent)" />
        <strong>{ctx.version}</strong>
      </div>
      <p class="lead small">
        {tr(
          "Your themes, extensions, settings and backups stay. The auto repair pauses during the update, then starts again by itself.",
          "Tes thèmes, extensions, réglages et sauvegardes sont gardés. La réparation auto est mise en pause le temps de la mise à jour, puis relancée toute seule.",
        )}
      </p>
    </div>
  {:else if screen === "newer" && ctx}
    <div class="pad enter">
      <div class="head">
        {@render sign("info", Info)}
        <div>
          <h1>
            {tr(
              "A newer version is already installed",
              "Une version plus récente est déjà installée",
            )}
          </h1>
          <p class="sub">
            {tr(
              "Nothing to do: spacetify is up to date. Installing this one would replace the newer version with an older one.",
              "Tu n'as rien à faire : spacetify est déjà à jour. Installer cette version-ci remplacerait la plus récente par une plus ancienne.",
            )}
          </p>
        </div>
      </div>
      <div class="pair">
        <div class="box">
          <span class="meta">{tr("Installed", "Installée")}</span>
          <strong class="version">{ctx.installedVersion}</strong>
        </div>
        <div class="box">
          <span class="meta">{tr("This installer", "Cet installateur")}</span>
          <strong class="version faint">{ctx.version}</strong>
        </div>
      </div>
      <p class="meta">
        {tr(
          "If you go back, your mods and settings stay, but some may no longer work.",
          "Si tu reviens en arrière, tes mods et réglages sont gardés, mais certains pourraient ne plus marcher.",
        )}
      </p>
    </div>
  {:else if screen === "uninstall"}
    <div class="pad enter">
      <div>
        <h1>{tr("Uninstall spacetify?", "Désinstaller spacetify ?")}</h1>
        <p class="sub">
          {tr(
            "The app, the terminal command and the auto repair are removed from this computer.",
            "L'appli, la commande du terminal et la réparation auto seront retirées de l'ordinateur.",
          )}
        </p>
      </div>
      <label class="option">
        <input type="checkbox" bind:checked={restore} />
        <span>
          <strong>{tr("Put Spotify back as it was", "Remettre Spotify d'origine")}</strong>
          <span class="detail">
            {tr(
              "Themes and extensions are removed from Spotify (it is closed if it is open). Otherwise Spotify keeps them until its next update.",
              "Les thèmes et extensions sont retirés de Spotify (il sera fermé s'il est ouvert). Sinon, Spotify les garde jusqu'à sa prochaine mise à jour.",
            )}
          </span>
        </span>
      </label>
      <p class="reassure">
        <ShieldCheck size={16} strokeWidth={1.5} color="var(--good)" />
        <span>
          {tr(
            "Your mods and backups stay in their folders either way: nothing is lost if you reinstall later.",
            "Tes mods et tes sauvegardes restent dans leurs dossiers dans tous les cas : rien n'est perdu si tu réinstalles plus tard.",
          )}
        </span>
      </p>
    </div>
  {:else if screen === "running"}
    <div class="pad enter">
      <div>
        <h1>{tr("spacetify is open", "spacetify est ouvert")}</h1>
        <p class="sub">
          {tr(
            "It has to close to go on. What you did in it is already saved.",
            "Il faut le fermer pour continuer. Ce que tu as fait dedans est déjà enregistré.",
          )}
        </p>
      </div>
      <div class="list">
        {#each programs as program (program)}
          {@const label = programLabel(program)}
          <div class="box row-box">
            {@render logo(28)}
            <span class="grow">
              <strong>{label.name}</strong>
              <span class="meta">{label.detail}</span>
            </span>
            <span class="live"><span class="dot good"></span>{tr("Running", "En cours")}</span>
          </div>
        {/each}
      </div>
      {#if closeFailed}
        <p class="reassure warn">
          <TriangleAlert size={16} strokeWidth={1.5} />
          <span>
            {tr(
              "spacetify could not be closed. Close it yourself (right-click its icon next to the clock, then Quit), then try again.",
              "spacetify n'a pas pu être fermé. Ferme-le toi-même (clic droit sur son icône près de l'horloge, puis Quitter), puis réessaie.",
            )}
          </span>
        </p>
      {:else}
        <p class="reassure">
          <Info size={16} strokeWidth={1.5} />
          <span>
            {tr(
              "The auto repair (in the background) is stopped and started again by itself: nothing to do for it.",
              "La réparation auto (en arrière-plan) est arrêtée et relancée toute seule : tu n'as rien à faire pour elle.",
            )}
          </span>
        </p>
      {/if}
    </div>
  {:else if screen === "progress"}
    <div class="pad wide">
      <div class="head">
        {@render logo(44)}
        <div>
          <h1>{progressTitle()}</h1>
          <p class="sub">{tr("This only takes a few seconds.", "Ça ne prend que quelques secondes.")}</p>
        </div>
      </div>
      <div class="progress">
        <div
          class="track"
          role="progressbar"
          aria-label={progressTitle()}
          aria-valuenow={percent}
          aria-valuemin="0"
          aria-valuemax="100"
        >
          <div class="fill" style:width="{percent}%"></div>
        </div>
        <div class="progress-text">
          <span>{active ? `${stepLabel(active)}…` : ""}</span>
          <span>{percent} %</span>
        </div>
      </div>
      <StepList {steps} {states} />
    </div>
  {:else if screen === "done"}
    <div class="center enter">
      <div class="tick" aria-hidden="true"><Check size={28} strokeWidth={1.75} /></div>
      {#if op.kind === "repair"}
        <h1 class="big">{tr("spacetify is repaired", "spacetify est réparé")}</h1>
        <p class="lead small">
          {tr(
            "Everything is back in place. Your mods and settings did not move.",
            "Tout est remis en place. Tes mods et réglages n'ont pas bougé.",
          )}
        </p>
      {:else if op.kind === "update"}
        <h1 class="big">{tr("spacetify is up to date", "spacetify est à jour")}</h1>
        <p class="lead small">
          {tr(
            `Version ${ctx?.version}. Your themes, extensions and settings did not move.`,
            `Version ${ctx?.version}. Tes thèmes, extensions et réglages n'ont pas bougé.`,
          )}
        </p>
      {:else}
        <h1 class="big">{tr("spacetify is installed", "spacetify est installé")}</h1>
        <p class="lead small">
          {tr(
            "Open it to pick your themes and extensions. It is also in the Start menu and on the desktop.",
            "Ouvre-le pour choisir tes thèmes et tes extensions. Tu le retrouves aussi dans le menu Démarrer et sur le bureau.",
          )}
        </p>
      {/if}
    </div>
  {:else if screen === "uninstalled"}
    <div class="center enter">
      <div class="tick" aria-hidden="true"><Check size={28} strokeWidth={1.75} /></div>
      <h1 class="big">{tr("spacetify is uninstalled", "spacetify est désinstallé")}</h1>
      <p class="lead small">
        {tr(
          "Your mods and backups are still there, in case you reinstall it.",
          "Tes mods et tes sauvegardes sont toujours là, au cas où tu le réinstallerais.",
        )}
      </p>
    </div>
  {:else if screen === "partial"}
    <div class="pad wide enter">
      <div>
        <h1>
          {failed.length > 1
            ? tr("Installed, but some steps did not work", "Installé, mais des étapes n'ont pas marché")
            : tr("Installed, but a step did not work", "Installé, mais une étape n'a pas marché")}
        </h1>
        <p class="sub">
          {tr(
            "spacetify works. Only what is marked below failed: you can run it again.",
            "spacetify fonctionne. Seul ce qui est marqué ci-dessous a raté : tu peux le relancer.",
          )}
        </p>
      </div>
      <StepList
        steps={["files", "path", "notifications", "autostart"]}
        {states}
        notes={Object.fromEntries(failed.map((item) => [item.step, stepConsequence(item.step)]))}
      />
    </div>
  {:else if screen === "error" && failure}
    <div class="pad enter">
      {#if failedStep === "restore"}
        <div class="head">
          {@render sign("warn", TriangleAlert)}
          <div>
            <h1>{tr("Spotify could not be put back", "Spotify n'a pas pu être remis d'origine")}</h1>
            <p class="sub">
              {tr(
                "Nothing was removed yet. Try again, or uninstall without touching Spotify.",
                "spacetify n'a encore rien retiré. Tu peux réessayer, ou désinstaller sans toucher à Spotify.",
              )}
            </p>
          </div>
        </div>
      {:else if failure.kind === "diskFull"}
        <div class="head">
          {@render sign("warn", HardDrive)}
          <div>
            <h1>{tr("Not enough space on the disk", "Pas assez de place sur le disque")}</h1>
            <p class="sub">
              {tr(
                `Free some space on the ${drive} disk (recycle bin, downloads…), then try again. Nothing was half installed.`,
                `Libère un peu de place sur le disque ${drive} (corbeille, téléchargements…), puis réessaie. Rien n'a été installé à moitié.`,
              )}
            </p>
          </div>
        </div>
        <div class="field">
          <span class="meta">{tr("Install folder", "Dossier d'installation")}</span>
          <div class="code">{ctx?.dir}</div>
        </div>
      {:else if failure.kind === "blocked"}
        <div class="head">
          {@render sign("warn", TriangleAlert)}
          <div>
            <h1>{tr("A file was blocked", "Un fichier a été bloqué")}</h1>
            <p class="sub">
              {op.kind === "uninstall"
                ? tr(
                    "Windows refused to remove a file of spacetify. Your antivirus may be checking it: try again in a moment.",
                    "Windows a refusé de supprimer un fichier de spacetify. Ton antivirus l'analyse peut-être : réessaie dans un instant.",
                  )
                : tr(
                    "Your antivirus has probably set a file of spacetify aside by mistake. Allow spacetify in your antivirus, then try again. Nothing was changed in Spotify.",
                    "Ton antivirus a sûrement mis un fichier de spacetify de côté par erreur. Autorise spacetify dans ton antivirus, puis réessaie. Rien n'a été modifié dans Spotify.",
                  )}
            </p>
          </div>
        </div>
        <div class="code">{tr("Access denied", "Accès refusé")} : {failure.file}</div>
        <button class="link start" onclick={() => openUrl(ANTIVIRUS)}>
          {tr("Why does this happen?", "Pourquoi ça arrive ?")}
        </button>
      {:else if failure.kind === "inUse"}
        <div class="head">
          {@render sign("warn", TriangleAlert)}
          <div>
            <h1>{tr("A file is busy", "Un fichier est occupé")}</h1>
            <p class="sub">
              {tr(
                "Another program is using a file of spacetify. Close it, then try again.",
                "Un autre programme utilise un fichier de spacetify. Ferme-le, puis réessaie.",
              )}
            </p>
          </div>
        </div>
        <div class="code">{failure.file}</div>
      {:else}
        <div class="head">
          {@render sign("warn", TriangleAlert)}
          <div>
            <h1>
              {op.kind === "uninstall"
                ? tr("The uninstall could not finish", "La désinstallation n'a pas pu finir")
                : tr("The install could not finish", "L'installation n'a pas pu finir")}
            </h1>
            <p class="sub">
              {op.kind === "uninstall"
                ? tr("Try again in a moment.", "Réessaie dans un instant.")
                : tr(
                    "Nothing was half installed. Try again in a moment.",
                    "Rien n'a été installé à moitié. Réessaie dans un instant.",
                  )}
            </p>
          </div>
        </div>
      {/if}
      {#if failure.kind === "other"}
        <div class="code">{failure.message}</div>
      {/if}
    </div>
  {/if}

  {#snippet footer()}
    {#if screen === "welcome" && ctx}
      <span class="meta">
        Version {ctx.version} ·
        <button class="link small" onclick={() => openUrl(LICENSE)}>{tr("GPL-3.0 license", "Licence GPL-3.0")}</button>
      </span>
      <span class="spacer"></span>
      <button class="btn" onclick={() => api.quit()}>{tr("Cancel", "Annuler")}</button>
      <button class="btn primary" onclick={() => begin({ kind: "install" })}>
        {tr("Install", "Installer")}
      </button>
    {:else if screen === "maintenance"}
      <button class="link" onclick={launchAndQuit}>{tr("Open spacetify", "Ouvrir spacetify")}</button>
      <span class="spacer"></span>
      <button class="btn" onclick={() => api.quit()}>{tr("Cancel", "Annuler")}</button>
      <button
        class="btn primary"
        onclick={() => (choice === "repair" ? begin({ kind: "repair" }) : (screen = "uninstall"))}
      >
        {tr("Continue", "Continuer")}
      </button>
    {:else if screen === "update"}
      <button class="link" onclick={() => openUrl(RELEASES)}>{tr("What's new", "Voir les nouveautés")}</button>
      <span class="spacer"></span>
      <button class="btn" onclick={() => api.quit()}>{tr("Later", "Plus tard")}</button>
      <button class="btn primary" onclick={() => begin({ kind: "update" })}>
        {tr("Update", "Mettre à jour")}
      </button>
    {:else if screen === "newer"}
      <button class="btn" onclick={() => begin({ kind: "downgrade" })}>
        {tr("Install anyway", "Installer quand même")}
      </button>
      <button class="btn primary" onclick={launchAndQuit}>{tr("Open spacetify", "Ouvrir spacetify")}</button>
    {:else if screen === "uninstall"}
      <button
        class="btn"
        onclick={() => (uninstallMode ? api.quit() : (screen = "maintenance"))}
      >
        {tr("Cancel", "Annuler")}
      </button>
      <button class="btn destructive" onclick={() => begin({ kind: "uninstall", restore })}>
        {tr("Uninstall", "Désinstaller")}
      </button>
    {:else if screen === "running"}
      <button class="btn" disabled={closing} onclick={() => api.quit()}>{tr("Cancel", "Annuler")}</button>
      {#if closeFailed}
        <button class="btn primary" onclick={() => begin(op)}>{tr("Try again", "Réessayer")}</button>
      {:else}
        <button class="btn primary" disabled={closing} onclick={closeAndContinue}>
          {closing
            ? tr("Closing…", "Fermeture…")
            : tr("Close spacetify and continue", "Fermer spacetify et continuer")}
        </button>
      {/if}
    {:else if screen === "done"}
      <label class="check">
        <input type="checkbox" bind:checked={openApp} />
        {tr("Open spacetify now", "Ouvrir spacetify maintenant")}
      </label>
      <span class="spacer"></span>
      <button class="btn primary" onclick={finish}>{tr("Finish", "Terminer")}</button>
    {:else if screen === "uninstalled"}
      <button class="btn primary" onclick={() => api.quit()}>{tr("Close", "Fermer")}</button>
    {:else if screen === "partial"}
      <button class="link" onclick={() => api.openLog()}>{tr("See the log", "Voir le journal")}</button>
      <span class="spacer"></span>
      <button class="btn" disabled={retrying} onclick={() => (screen = "done")}>
        {tr("Finish anyway", "Terminer quand même")}
      </button>
      <button class="btn primary" disabled={retrying} onclick={retryFailed}>
        {retrying
          ? tr("Trying…", "Nouvel essai…")
          : failed.length > 1
            ? tr("Try these steps again", "Réessayer ces étapes")
            : tr("Try this step again", "Réessayer cette étape")}
      </button>
    {:else if screen === "error"}
      <button class="link" onclick={() => api.openLog()}>{tr("See the full log", "Voir le journal complet")}</button>
      <span class="spacer"></span>
      {#if failedStep === "restore"}
        <button class="btn" onclick={() => begin({ kind: "uninstall", restore: false })}>
          {tr("Uninstall anyway", "Désinstaller quand même")}
        </button>
      {:else}
        <button class="btn" onclick={() => api.quit()}>{tr("Close", "Fermer")}</button>
      {/if}
      <button class="btn primary" onclick={() => begin(op)}>{tr("Try again", "Réessayer")}</button>
    {/if}
  {/snippet}

  {#snippet overlay()}
    {#if askStop}
      <div class="scrim">
        <div class="dialog enter" role="dialog" aria-modal="true" aria-labelledby="stop-title">
          <h2 id="stop-title">{tr("Stop the install?", "Arrêter l'installation ?")}</h2>
          <p class="sub">
            {tr(
              "What was already copied is removed: your computer stays as it was. Spotify is not touched.",
              "Ce qui a déjà été copié sera retiré : ton ordinateur restera comme avant. Spotify n'est pas touché.",
            )}
          </p>
          <div class="dialog-buttons">
            <button class="btn" onclick={stop}>{tr("Stop", "Arrêter")}</button>
            <button class="btn primary" onclick={() => (askStop = false)}>
              {tr("Continue the install", "Continuer l'installation")}
            </button>
          </div>
        </div>
      </div>
    {/if}
  {/snippet}
</Frame>

<style>
  h1 {
    font-size: 18px;
    line-height: 24px;
    font-weight: 600;
  }

  h1.name {
    margin-top: 16px;
    font-size: 28px;
    line-height: 34px;
    font-weight: 700;
    letter-spacing: -0.02em;
  }

  h1.big {
    margin-top: 16px;
    font-size: 22px;
    line-height: 28px;
    font-weight: 700;
    letter-spacing: -0.01em;
  }

  .sub {
    margin-top: 4px;
    font-size: 13px;
    color: var(--text-2);
  }

  .center {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 24px 56px;
    text-align: center;
  }

  .lead {
    margin-top: 8px;
    max-width: 400px;
    color: var(--text-2);
  }

  .lead.small {
    font-size: 13px;
  }

  .pad {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 18px;
    padding: 28px 40px;
  }

  .pad.wide {
    gap: 22px;
  }

  .head {
    display: flex;
    align-items: center;
    gap: 14px;
  }

  .head:has(.sign) {
    align-items: flex-start;
  }

  .sign {
    width: 40px;
    height: 40px;
    flex: none;
    border-radius: 10px;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .sign.warn {
    background: rgba(229, 168, 59, 0.12);
    color: var(--warn);
  }

  .sign.info {
    background: rgba(237, 237, 237, 0.06);
    color: var(--text-2);
  }

  .pill {
    margin-top: 20px;
    display: flex;
    align-items: center;
    gap: 8px;
    height: 28px;
    padding: 0 12px;
    border-radius: 999px;
    background: var(--surface);
    border: 1px solid var(--line);
    font-size: 12px;
    color: var(--text-2);
  }

  .versions {
    margin-top: 12px;
    display: flex;
    align-items: center;
    gap: 10px;
    height: 30px;
    padding: 0 14px;
    border-radius: 999px;
    background: var(--surface);
    border: 1px solid var(--line);
    font-size: 13px;
  }

  .faint {
    color: var(--text-3);
  }

  .options {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .option {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 12px 14px;
    border-radius: 10px;
    border: 1px solid var(--line);
    background: var(--surface);
    cursor: pointer;
    transition:
      border-color 150ms var(--ease),
      background-color 150ms var(--ease);
  }

  .option.chosen {
    border-color: rgba(255, 107, 129, 0.5);
    background: rgba(255, 107, 129, 0.06);
  }

  .option > span {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .option strong {
    font-weight: 600;
  }

  .detail {
    font-size: 12px;
    line-height: 18px;
    color: var(--text-2);
  }

  input[type="radio"],
  input[type="checkbox"] {
    width: 16px;
    height: 16px;
    margin: 2px 0 0;
    flex: none;
    accent-color: var(--accent);
    cursor: pointer;
  }

  .check {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 13px;
    cursor: pointer;
  }

  .check input {
    margin: 0;
  }

  .pair {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 8px;
  }

  .box {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 12px 14px;
    border-radius: 10px;
    background: var(--surface);
    border: 1px solid var(--line);
  }

  .version {
    font-size: 16px;
    font-weight: 600;
  }

  .list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .row-box {
    flex-direction: row;
    align-items: center;
    gap: 12px;
  }

  .grow {
    flex: 1;
    display: flex;
    flex-direction: column;
  }

  .live {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text-2);
  }

  .reassure {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    font-size: 12px;
    line-height: 18px;
    color: var(--text-3);
  }

  .reassure :global(svg) {
    flex: none;
    margin-top: 1px;
  }

  .reassure.warn {
    color: var(--text-2);
  }

  .reassure.warn :global(svg) {
    color: var(--warn);
  }

  .progress {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .track {
    height: 6px;
    border-radius: 999px;
    background: var(--raised);
    overflow: hidden;
  }

  .fill {
    height: 100%;
    border-radius: 999px;
    background: linear-gradient(90deg, #ffa273, #ff6b81 60%, #e5487f);
    transition: width 200ms var(--ease);
  }

  .progress-text {
    display: flex;
    justify-content: space-between;
    font-size: 12px;
    color: var(--text-3);
  }

  .tick {
    width: 64px;
    height: 64px;
    border-radius: 999px;
    background: var(--accent-soft);
    border: 1px solid rgba(255, 107, 129, 0.3);
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--accent);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .code {
    padding: 12px 14px;
    border-radius: 8px;
    background: var(--surface);
    border: 1px solid var(--line);
    font-family: var(--mono);
    font-size: 12px;
    line-height: 18px;
    color: var(--text-2);
    overflow-wrap: anywhere;
    max-height: 72px;
    overflow-y: auto;
  }

  .start {
    align-self: flex-start;
  }

  .link.small {
    font-size: 12px;
  }

  .scrim {
    position: absolute;
    inset: 40px 0 0 0;
    background: rgba(8, 8, 9, 0.72);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
  }

  .dialog {
    width: 380px;
    box-sizing: border-box;
    padding: 22px;
    border-radius: 12px;
    background: #1a1a1d;
    border: 1px solid var(--line-strong);
    box-shadow: 0 16px 40px rgba(0, 0, 0, 0.5);
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .dialog h2 {
    font-size: 16px;
    line-height: 22px;
    font-weight: 600;
  }

  .dialog-buttons {
    margin-top: 12px;
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  .dialog-buttons .btn {
    height: 40px;
    padding: 0 16px;
  }

  .dialog-buttons .btn.primary {
    font-weight: 700;
  }
</style>
