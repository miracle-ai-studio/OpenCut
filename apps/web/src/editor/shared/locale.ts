import { useSyncExternalStore } from "react";
import { zh } from "./messages";
export type Language = "zh" | "en";
export type Theme = "light" | "dark";
const read = (key: string) => {
  try {
    return window.localStorage.getItem(key);
  } catch {
    return null;
  }
};
let language: Language =
  read("opencut.language") === "en"
    ? "en"
    : read("opencut.language") === "zh"
      ? "zh"
      : typeof navigator !== "undefined" && navigator.language.startsWith("zh")
        ? "zh"
        : "en";
let theme: Theme = read("opencut.theme") === "light" ? "light" : "dark";
const listeners = new Set<() => void>();
const subscribe = (listener: () => void) => {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
};
function apply() {
  if (typeof document !== "undefined") {
    document.documentElement.lang = language === "zh" ? "zh-CN" : "en";
    document.documentElement.dataset.theme = theme;
  }
}
apply();
export function preferences(next: { language?: Language; theme?: Theme }) {
  language = next.language ?? language;
  theme = next.theme ?? theme;
  try {
    window.localStorage.setItem("opencut.language", language);
    window.localStorage.setItem("opencut.theme", theme);
  } catch {
    /* Browser storage is optional. */
  }
  apply();
  listeners.forEach((listener) => listener());
}
export function useLocale() {
  useSyncExternalStore(subscribe, () => `${language}:${theme}`);
  return { language, theme, t };
}
export function t(
  message: string,
  values: Record<string, string | number> = {},
) {
  const text = language === "zh" ? (zh[message] ?? message) : message;
  return text.replace(/\{(\w+)\}/g, (match, name) =>
    String(values[name] ?? match),
  );
}

export function parameterLabel(name: string) {
  return t(
    (
      {
        position_x: "Position X",
        position_y: "Position Y",
        scale_x: "Scale X",
        scale_y: "Scale Y",
        rotation_degrees: "Rotation",
        opacity: "Opacity",
        volume: "Volume",
      } as Record<string, string>
    )[name] ?? name.replaceAll("_", " "),
  );
}
