import type { Effect } from "../scene-stack/types";
import type { ResourceCopyKey } from "./resources-copy";
/** Parameter presets of the executable Scene blur, not additional effect kinds. */
export const blurPresets: readonly {
  name: ResourceCopyKey;
  intensity: number;
}[] = [
  { name: "soft", intensity: 5 },
  { name: "medium", intensity: 15 },
  { name: "strong", intensity: 35 },
];
export const transitionPresets: readonly {
  name: ResourceCopyKey;
  seconds: string;
}[] = [
  { name: "quick", seconds: "0.25" },
  { name: "smooth", seconds: "0.5" },
  { name: "slow", seconds: "1" },
];
export function presetBlur(intensity: number): Effect {
  return {
    id: crypto.randomUUID(),
    enabled: true,
    effect: { kind: "classic_blur", intensity },
  };
}
