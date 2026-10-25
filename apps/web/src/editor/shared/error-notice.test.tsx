// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { ErrorNotice } from "./error-notice";
import { preferences } from "./locale";
afterEach(cleanup);
it.each([
  ["Confirm or cancel the property draft before changing selection.", "请先确认或取消属性草稿，再选择其它对象。"],
  ["Confirm or cancel the property draft before starting another edit.", "请先确认或取消属性草稿，再开始其它编辑。"],
])("shows the actionable draft instruction for %s", (message, expected) => {
  preferences({ language: "zh" });
  render(<ErrorNotice error={new Error(message)} />);
  expect(screen.getByRole("alert").textContent).toBe(expected);
});
it.each([true, false])("explains rejected authorization in Chinese and preserves the exact diagnostic (local=%s)", localSession => {
  preferences({ language: "zh" });
  render(<ErrorNotice error={new Error("A valid bearer token is required")} localSession={localSession} />);
  expect(screen.getByRole("alert").textContent).toBe(localSession
    ? "本机会话已过期或服务已重启，请重新连接。已保存的编辑仍然保留。"
    : "连接凭证被拒绝，请使用有效凭证重新连接原服务。已保存的编辑仍然保留。");
  expect(screen.getByText("A valid bearer token is required").closest("details")).toBeTruthy();
});
