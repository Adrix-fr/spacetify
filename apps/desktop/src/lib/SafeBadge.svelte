<script lang="ts">
  // "Safe mode": whether an installed mod is exactly what the gallery accepted.
  import { ShieldCheck, ShieldAlert } from "@lucide/svelte";
  import { tr } from "./state.svelte";

  let { verified }: { verified: boolean } = $props();

  const why = $derived(
    verified
      ? tr(
          "Verified: exactly as the gallery accepted it, after the ClamAV antivirus, spacetify's robot and the team's review.",
          "Vérifié : exactement tel que la galerie l'a accepté, après l'antivirus ClamAV, le robot de spacetify et la relecture de l'équipe.",
        )
      : tr(
          "Not verified: installed by hand or changed since, it did not go through the gallery's checks.",
          "Non vérifié : installé à la main ou modifié depuis, il n'est pas passé par les vérifications de la galerie.",
        ),
  );
</script>

<span class="badge" class:verified title={why} aria-label={why}>
  {#if verified}
    <ShieldCheck size={12} strokeWidth={2} />{tr("Verified", "Vérifié")}
  {:else}
    <ShieldAlert size={12} strokeWidth={2} />{tr("Not verified", "Non vérifié")}
  {/if}
</span>

<style>
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 20px;
    padding: 0 8px;
    border-radius: 10px;
    font-size: 11px;
    font-weight: 600;
    white-space: nowrap;
    vertical-align: middle;
    color: var(--warn);
    background: color-mix(in srgb, var(--warn) 12%, transparent);
    cursor: default;
  }
  .badge.verified {
    color: var(--good);
    background: color-mix(in srgb, var(--good) 12%, transparent);
  }
</style>
