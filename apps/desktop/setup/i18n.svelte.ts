// Language of the installer window: spacetify's own setting, else the system's.
export const ui = $state({ lang: "en" as "en" | "fr" });

export function tr(en: string, fr: string): string {
  return ui.lang === "fr" ? fr : en;
}
