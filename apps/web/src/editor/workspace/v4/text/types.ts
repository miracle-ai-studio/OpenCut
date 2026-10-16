import type { V4Document, V4Element } from "../types";
import type { FontSynthesisBinding } from "./font-synthesis";

export type V4Text = Extract<V4Element["content"], { kind: "text" }>["text"];
/** Read-only UI enrichment; actual resource fields are never relabeled. */
export type V4TextFont = V4Document["fonts"][number] & {
  requested_style?: "normal" | "italic";
  synthesis_binding?: FontSynthesisBinding;
  /** From a validated synthesis base proof, for UI grouping only. */
  display_family?: string;
};
export type V4TextInitial = Pick<V4Element, "name" | "timeline_range"> & {
  text: V4Text;
};
/** Form input, not a wire DTO. Unchanged time retains the original rational;
 * edited decimal text is parsed by the application planner, never by the UI. */
export type V4TextTimeInput = V4Element["timeline_range"]["start"] | string;
export type V4TextEditorValue = Pick<V4TextInitial, "name" | "text"> & {
  timeline_range: { start: V4TextTimeInput; duration: V4TextTimeInput };
};
export type V4TextScope = {
  projectId: string;
  revision: string;
  sequenceId: string;
  elementId?: string;
};
export type V4TextEditorProps = {
  mode: "create" | "edit";
  scope: V4TextScope;
  /** Defaults come from the application; editing uses the authored document. */
  initial: V4TextInitial;
  fonts: readonly V4TextFont[];
  /** True only after the host confirms executable locked-font frame support. */
  available: boolean;
  disabled?: boolean;
  disabledReason?: string | null;
  save(value: V4TextEditorValue): Promise<unknown>;
  close(): void;
};
