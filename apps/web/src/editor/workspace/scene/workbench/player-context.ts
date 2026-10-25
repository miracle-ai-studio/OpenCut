import { createContext, useContext } from "react";
import type { Asset } from "../types";

export type PlayerPreview =
  | { kind: "source"; asset: Asset; projectId: string; revision: string }
  | { kind: "effect"; name: string; intensity: number }
  | { kind: "transition"; name: string; seconds: string };

/** Ephemeral presentation and playback ownership, never project state. */
export const PlayerContext = createContext<{
  preview: PlayerPreview | null;
  open(preview: PlayerPreview): Promise<void>;
  close(restoreFocus?: boolean): void;
  begin(owner: symbol, pause: () => void): Promise<boolean>;
  release(owner: symbol): void;
  stopExternal(): void;
} | null>(null);
export const usePlayerContext = () => useContext(PlayerContext);
