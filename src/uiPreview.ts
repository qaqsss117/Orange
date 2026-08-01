export type PreviewTheme = "system" | "light" | "dark";
export type PreviewFontScale = "normal" | "large";
export type PreviewMotion = "full" | "reduced";

const THEME_STORAGE_KEY = "orange.theme";
const THEMES: readonly PreviewTheme[] = ["system", "light", "dark"];

export interface UiPreviewConfiguration {
  theme: PreviewTheme;
  fontScale: PreviewFontScale;
  motion: PreviewMotion;
}

function oneOf<T extends string>(
  value: string | null,
  allowed: readonly T[],
  fallback: T,
): T {
  return value !== null && allowed.includes(value as T)
    ? (value as T)
    : fallback;
}

export function readUiPreview(search: string): UiPreviewConfiguration {
  const parameters = new URLSearchParams(search);
  return {
    theme: oneOf(
      parameters.get("theme"),
      ["system", "light", "dark"],
      "system",
    ),
    fontScale: oneOf(parameters.get("scale"), ["normal", "large"], "normal"),
    motion: oneOf(parameters.get("motion"), ["full", "reduced"], "full"),
  };
}

export function readThemePreference(search: string): PreviewTheme {
  const configured = new URLSearchParams(search).get("theme");
  if (configured !== null) {
    return oneOf(configured, THEMES, "system");
  }
  try {
    return oneOf(
      window.localStorage.getItem(THEME_STORAGE_KEY),
      THEMES,
      "system",
    );
  } catch {
    return "system";
  }
}

export function storeThemePreference(theme: PreviewTheme): void {
  try {
    window.localStorage.setItem(THEME_STORAGE_KEY, theme);
  } catch {
    // The selected theme still applies for the current session.
  }
}

export function systemTheme(): Exclude<PreviewTheme, "system"> {
  return window.matchMedia?.("(prefers-color-scheme: dark)").matches
    ? "dark"
    : "light";
}
