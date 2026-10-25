import { describe, expect, it } from "vitest";
import { formatHistoryDate } from "./history-copy";

describe("history date display", () => {
  it("keeps the year and viewer timezone in a compact row, with full precision available separately", () => {
    expect(formatHistoryDate("2026-10-21 16:43:38", "zh", "Asia/Shanghai", true)).toBe("2026/10/22 00:43");
    expect(formatHistoryDate("2026-10-21 16:43:38", "en", "UTC", true)).toBe("10/21/2026, 16:43");
  });
  it("interprets SQLite's unzoned UTC timestamp correctly across the Shanghai date boundary", () => {
    expect(formatHistoryDate("2026-10-21 16:43:38", "zh", "Asia/Shanghai")).toBe("2026/10/22 00:43:38");
    expect(formatHistoryDate("2026-10-21 16:43:38.125", "en", "Asia/Shanghai")).toBe("10/22/2026, 12:43:38 AM");
  });

  it("preserves existing RFC3339 offsets and UTC designators", () => {
    const expected = "2026/10/22 00:43:38";
    for (const timestamp of ["2026-10-21T16:43:38Z", "2026-10-22T00:43:38+08:00", "2026-10-21T09:43:38-07:00"])
      expect(formatHistoryDate(timestamp, "zh", "Asia/Shanghai")).toBe(expected);
  });

  it("uses the viewer's local timezone when no test timezone is specified", () => {
    expect(formatHistoryDate("2026-10-21 16:43:38", "en")).toBe(new Date("2026-10-21T16:43:38Z").toLocaleString("en-US"));
    expect(formatHistoryDate("2026-10-21 16:43:38", "en", "UTC")).toBe("10/21/2026, 4:43:38 PM");
  });

  it("does not expose an invalid date string", () => {
    expect(formatHistoryDate("not-a-date", "zh", "Asia/Shanghai")).toBe("时间暂不可用");
    expect(formatHistoryDate("", "en", "Asia/Shanghai")).toBe("Time unavailable");
  });
});
