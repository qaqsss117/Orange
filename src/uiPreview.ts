export type PreviewTheme = "system" | "light" | "dark";
export type PreviewFontScale = "normal" | "large";
export type PreviewMotion = "full" | "reduced";

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

export function systemTheme(): Exclude<PreviewTheme, "system"> {
  return window.matchMedia?.("(prefers-color-scheme: dark)").matches
    ? "dark"
    : "light";
}
