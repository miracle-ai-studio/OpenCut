import type { Document, Element } from "../types";
import type { FontSynthesisBinding } from "./font-synthesis";

export type Text = Extract<Element["content"], { kind: "text" }>["text"];
/** Read-only UI enrichment; actual resource fields are never relabeled. */
export type TextFont = Document["fonts"][number] & {
  requested_style?: "normal" | "italic";
  synthesis_binding?: FontSynthesisBinding;
  /** From a validated synthesis base proof, for UI grouping only. */
  display_family?: string;
};
export type TextInitial = Pick<Element, "name" | "timeline_range"> & {
  text: Text;
};
/** Form input, not a wire DTO. Unchanged time retains the original rational;
 * edited decimal text is parsed by the application planner, never by the UI. */
export type TextTimeInput = Element["timeline_range"]["start"] | string;
export type TextEditorValue = Pick<TextInitial, "name" | "text"> & {
  timeline_range: { start: TextTimeInput; duration: TextTimeInput };
};
export type TextScope = {
  projectId: string;
  revision: string;
  sequenceId: string;
  elementId?: string;
};
export type TextEditorProps = {
  mode: "create" | "edit";
  scope: TextScope;
  /** Defaults come from the application; editing uses the authored document. */
  initial: TextInitial;
  fonts: readonly TextFont[];
  /** True only after the host confirms executable locked-font frame support. */
  available: boolean;
  disabled?: boolean;
  disabledReason?: string | null;
  save(value: TextEditorValue): Promise<unknown>;
  close(): void;
};
