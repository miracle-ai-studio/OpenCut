// @vitest-environment jsdom
import { expect, it, vi } from "vitest";
import {
  assertAssReviewContext,
  type FrozenClassicAssReview,
  type AssReviewLiveContext,
} from "./workspace";
import type { EditorApi } from "../../api";
vi.mock("../../context", () => ({ useWorkspace: vi.fn(), useEditingWorkspace: vi.fn() }));
function fixture() {
  const api = { connectionIdentity: "connection" } as unknown as EditorApi;
  const review = {
    api,
    connectionIdentity: "connection",
    scope: { project_id: "p", expected_revision: "r", sequence_id: "s" },
    request: {
      content: "exact source",
      font_bindings: [
        { family: "Face", weight: "normal", style: "normal", font_id: "f" },
      ],
    },
    trackChoice: "__new_subtitle_text_track__",
    aliases: JSON.stringify([["face", "f"]]),
    mode: "classic",
    format: "ass",
  } as Omit<FrozenClassicAssReview, "plan">;
  const live: AssReviewLiveContext = {
    api,
    formApi: api,
    connectionIdentity: "connection",
    formConnectionIdentity: "connection",
    historical: false,
    projectId: "p",
    revision: "r",
    sequenceId: "s",
    content: "exact source",
    trackChoice: "__new_subtitle_text_track__",
    aliases: { face: "f" },
    mode: "classic",
    format: "ass",
  };
  return { review, live };
}
it("permits only the original scope and source review", () => {
  const { review, live } = fixture();
  expect(() => assertAssReviewContext(review, live)).not.toThrow();
});
it.each([
  "content",
  "trackChoice",
  "aliases",
  "mode",
  "format",
  "formApi",
  "formConnectionIdentity",
  "historical",
] as const)("rejects changed %s independently of UI effect timing", (field) => {
  const { review, live } = fixture();
  const changed = {
    ...live,
    [field]:
      field === "aliases"
        ? { face: "other" }
        : field === "mode"
          ? "strict"
          : field === "format"
            ? "srt"
            : field === "formApi"
              ? {}
              : field === "historical"
                ? true
                : "changed",
  } as AssReviewLiveContext;
  expect(() => assertAssReviewContext(review, changed)).toThrow(
    "The subtitle review changed",
  );
});
