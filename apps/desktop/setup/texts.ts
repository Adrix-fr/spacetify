// Words shared by several screens.
import type { Step } from "./api";
import { tr } from "./i18n.svelte";

export type OpKind = "install" | "repair" | "update" | "downgrade" | "uninstall";

export interface Op {
  kind: OpKind;
  /** Uninstall only: put Spotify back as it was first. */
  restore?: boolean;
}

export const RELEASES = "https://github.com/Adrix-fr/spacetify/releases";
export const LICENSE = "https://github.com/Adrix-fr/spacetify/blob/main/LICENSE";
export const ANTIVIRUS =
  "https://github.com/Adrix-fr/spacetify#releases-and-antivirus-software";

export function stepLabel(step: Step): string {
  switch (step) {
    case "files":
      return tr("Copying the files", "Copie des fichiers");
    case "path":
      return tr("spacetify command in the terminal", "Commande spacetify dans le terminal");
    case "notifications":
      return tr("Notifications under spacetify's name", "Notifications à ton nom");
    case "autostart":
      return tr("Auto repair at Windows startup", "Réparation auto au démarrage de Windows");
    case "restore":
      return tr("Putting Spotify back as it was", "Remise de Spotify d'origine");
    case "undo":
      return tr("Stopping the auto repair", "Arrêt de la réparation auto");
    case "remove":
      return tr("Removing the files", "Suppression des fichiers");
  }
}

/** What is missing while a step after the files has failed. */
export function stepConsequence(step: Step): string {
  switch (step) {
    case "files":
      return tr(
        "The shortcuts or the entry in Windows settings are missing.",
        "Les raccourcis ou l'entrée dans les paramètres de Windows manquent.",
      );
    case "path":
      return tr(
        "The spacetify command does not work in the terminal yet.",
        "La commande spacetify ne marche pas encore dans le terminal.",
      );
    case "notifications":
      return tr(
        "Meanwhile, notifications show under the name “Windows PowerShell”.",
        "En attendant, les notifications s'affichent sous le nom « Windows PowerShell ».",
      );
    case "autostart":
      return tr(
        "The auto repair does not start with Windows yet.",
        "La réparation auto ne se lance pas encore au démarrage de Windows.",
      );
    default:
      return "";
  }
}

export function windowTitle(op: OpKind | null, uninstallMode: boolean): string {
  if (op === "uninstall" || uninstallMode) {
    return tr("Uninstall spacetify", "Désinstallation de spacetify");
  }
  if (op === "update") return tr("spacetify update", "Mise à jour de spacetify");
  return tr("spacetify setup", "Installation de spacetify");
}
