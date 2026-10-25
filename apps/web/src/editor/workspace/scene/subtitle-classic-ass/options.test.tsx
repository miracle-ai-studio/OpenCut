// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, it, expect, vi } from "vitest";
import { ClassicAssImportOptions } from "./options";
import { preferences } from "../../../shared/locale";
import type { ClassicAssPlan } from "./planner";

afterEach(cleanup);
it("keeps strict import selectable and source losses reviewable before saving", () => {
  preferences({ language: "en" });
  const mode = vi.fn(),
    save = vi.fn();
  const plan = {
    cues: [],
    skipped_cue_count: 2,
    warnings: [
      "Stripped unsupported ASS inline override tags from 1 subtitle cue(s).",
    ],
  } as unknown as ClassicAssPlan;
  render(
    <ClassicAssImportOptions
      mode="classic"
      onModeChange={mode}
      plan={plan}
      onAcceptLosses={save}
    />,
  );
  expect(screen.getByLabelText("ASS import review").textContent).toContain(
    "2 invalid or empty cues skipped",
  );
  expect(screen.getByText(plan.warnings[0])).toBeTruthy();
  expect(save).not.toHaveBeenCalled();
  fireEvent.click(screen.getByLabelText("Strict style preservation"));
  expect(mode).toHaveBeenCalledWith("strict");
  fireEvent.click(screen.getByText("Save reviewed subtitles"));
  expect(save).toHaveBeenCalledOnce();
});
it("localizes the review and original Classic loss counters in Chinese", () => {
  preferences({ language: "zh" });
  const plan = {
    cues: [],
    skipped_cue_count: 2,
    warnings: ["Ignored ASS event effects in 3 subtitle cue(s)."],
  } as unknown as ClassicAssPlan;
  const { unmount } = render(
    <ClassicAssImportOptions
      mode="classic"
      onModeChange={() => {}}
      plan={plan}
    />,
  );
  expect(screen.getByLabelText("ASS 导入审阅").textContent).toContain(
    "跳过 2 条无效或空字幕",
  );
  expect(screen.getByText("已忽略 3 条字幕中的 ASS 事件效果。")).toBeTruthy();
  unmount();
  preferences({ language: "en" });
});
