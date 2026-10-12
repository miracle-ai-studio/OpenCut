// @vitest-environment jsdom
import { beforeEach, expect, it, vi } from "vitest";
import { HistoryViewStore, type HistoryViews } from "./view-preferences";
const entry = (transactionId = "delete"): HistoryViews => ({
  host: "http://localhost:8788/",
  projectId: "p",
  transactionId,
  before: {
    sequenceId: "s",
    selections: [{ kind: "clip", trackId: "t", id: "c" }],
    playhead: { ticks: "2", timebase: 1 },
  },
  after: {
    sequenceId: "main",
    selections: [],
    playhead: { ticks: "0", timebase: 1 },
  },
});
beforeEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
});
it("persists owned bounded views across stores, scopes by host/project/transaction and merges unrelated tab entries", () => {
  const a = new HistoryViewStore(),
    b = new HistoryViewStore(),
    value = entry();
  a.save(value);
  value.before.selections[0].id = "modified";
  b.save(entry("another-tab"));
  expect(new HistoryViewStore().read(value.host, "p", "delete")).toEqual(
    entry(),
  );
  expect(a.read(value.host, "p", "another-tab")).toEqual(entry("another-tab"));
  expect(a.read("https://other/", "p", "delete")).toBeNull();
  expect(a.read(value.host, "q", "delete")).toBeNull();
  for (let i = 0; i < 129; i++) a.save(entry(String(i)));
  expect(new HistoryViewStore().read(value.host, "p", "0")).toBeNull();
  expect(new HistoryViewStore().read(value.host, "p", "128")).toBeTruthy();
});
it("rejects corrupt/oversized hints and projects only view fields", () => {
  const key = "opencut.history-views.v1",
    value = entry();
  localStorage.setItem(
    key,
    JSON.stringify([
      value,
      {
        ...entry("bad"),
        before: { ...value.before, playhead: { ticks: "NaN", timebase: 1 } },
      },
    ]),
  );
  const store = new HistoryViewStore();
  expect(store.read(value.host, "p", "bad")).toBeNull();
  store.save({ ...value, secret: "not persisted" } as HistoryViews);
  expect(localStorage.getItem(key)).not.toContain("secret");
  localStorage.setItem(key, "x".repeat(2_000_001));
  expect(new HistoryViewStore().read(value.host, "p", "delete")).toBeNull();
});
it("keeps current-session restoration available when storage is denied", () => {
  vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
    throw new Error("denied");
  });
  const store = new HistoryViewStore(),
    value = entry();
  store.save(value);
  expect(store.read(value.host, "p", "delete")).toEqual(value);
});
