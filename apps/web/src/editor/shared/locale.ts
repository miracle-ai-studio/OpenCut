import { friendlyCopy } from "./friendly-copy";
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
  const copy = friendlyCopy[message];
  const text = copy ? copy[language === "zh" ? 1 : 0] : language === "zh" ? (zh[message] ?? message) : message;
  return text.replace(/\{(\w+)\}/g, (match, name) =>
    String(values[name] ?? match),
  );
}

export function hasMessage(message: string) { return Object.hasOwn(zh, message) || Object.hasOwn(friendlyCopy, message); }

export function parameterLabel(name: string) {
  return t(
    (
      {
        "position_x_px": "Horizontal position",
        "position_y_px": "Vertical position",
        "gain_decibels": "Volume (dB)",
        "gain_linear_legacy": "Volume multiplier",
        "font_size_canvas_pixels": "Font size (px)",
        "font_size_classic_height90": "Font size relative to canvas",
        "letter_spacing_px": "Letter spacing",
        "line_height_multiplier": "Line spacing",
        "text_background_padding_x_at_size15": "Horizontal text padding",
        "text_background_padding_y_at_size15": "Vertical text padding",
        "text_background_offset_x_px": "Background horizontal offset",
        "text_background_offset_y_px": "Background vertical offset",
        "text_background_corner_radius_percent": "Background roundness",
        "graphic_stroke_width_px": "Outline width",
        "graphic_corner_radius_percent": "Corner roundness",
        "graphic_sides": "Sides",
        "graphic_points": "Points",
        "graphic_depth_percent": "Inner depth",
        "classic_blur_intensity": "Blur amount",
        "legacy_blur_radius_px": "Blur radius",
        "legacy_exposure_ev": "Exposure",
        "legacy_contrast": "Contrast",
        "legacy_saturation": "Saturation",
        "mask_center_x": "Mask horizontal position",
        "mask_center_y": "Mask vertical position",
        "mask_width": "Mask width",
        "mask_height": "Mask height",
        "mask_rotation_degrees": "Mask rotation",
        "mask_scale": "Mask scale",
        "mask_feather_classic_pixels": "Feather (px)",
        "mask_feather_legacy_canvas_fraction": "Feather relative to canvas",
        "text_color": "Text color",
        "text_background_color": "Text background",
        "graphic_fill": "Shape fill",
        "graphic_stroke": "Shape outline",
        "mask_stroke": "Mask outline",
        "red": "Red",
        "green": "Green",
        "blue": "Blue",
        "alpha": "Opacity",
        "inside": "Inside",
        "center": "Center",
        "outside": "Outside",
        "graphic_stroke_align": "Shape outline position",
        "mask_stroke_align": "Mask outline position",
        position_x: "Position X",
        position_y: "Position Y",
        scale_x: "Scale X",
        scale_y: "Scale Y",
        rotation_degrees: "Rotation",
        opacity: "Opacity",
        volume: "Volume",
      } as Record<string, string>
    )[name] ?? (friendlyCopy[name] ? name : name.replaceAll("_", " ")),
  );
}

export function mediaKindLabel(kind: string) {
  return t(({ video: "Video", audio: "Audio", image: "Image", text: "Text", sticker: "Sticker", graphic: "Graphic", effect: "Effect", effect_layer: "Effect layer", caption: "Caption" } as Record<string, string>)[kind] ?? "Clip");
}

/** Localize generated split suffixes without rewriting saved names or files. */
export function elementLabel(name: string) {
  if(name==="Blur effect layer"||name==="Blur")return t(name);
  const match=/^(.*) \((left|right)\)$/.exec(name);
  return match?t(match[2]==="left"?"{name} (left)":"{name} (right)",{name:match[1]}):name;
}
