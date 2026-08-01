export type ThemePreference = "system" | "light" | "dark";

const THEME_STORAGE_KEY = "orange.theme";
const THEMES: readonly ThemePreference[] = ["system", "light", "dark"];

function parseTheme(value: string | null): ThemePreference {
  return value !== null && (THEMES as readonly string[]).includes(value)
    ? (value as ThemePreference)
    : "system";
}

export function readThemePreference(): ThemePreference {
  try {
    return parseTheme(window.localStorage.getItem(THEME_STORAGE_KEY));
  } catch {
    return "system";
  }
}

export function storeThemePreference(theme: ThemePreference): void {
  try {
    window.localStorage.setItem(THEME_STORAGE_KEY, theme);
  } catch {
    // The selected theme still applies for the current session.
  }
}

export function systemTheme(): Exclude<ThemePreference, "system"> {
  return window.matchMedia?.("(prefers-color-scheme: dark)").matches
    ? "dark"
    : "light";
}
