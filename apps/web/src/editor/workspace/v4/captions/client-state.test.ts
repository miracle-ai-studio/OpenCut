import { afterEach, expect, it, vi } from "vitest";
import {
  readCaptionRecoveryLocator,
  writeCaptionRecoveryLocator,
  clearCaptionRecoveryLocator,
  readTranscriptionRequest,
  writeTranscriptionRequest,
  clearTranscriptionRequest,
  type TranscriptionRequestV4,
} from "./client-state";
const connection = JSON.stringify([
  "http://localhost:8788",
  "editor",
  "credential",
  false,
]);
const original = (): TranscriptionRequestV4 => ({
  project_id: "project",
  revision: "fixed",
  sequence_id: "scene",
  idempotency_key: "original",
  language: "zh",
});
function storage() {
  const entries = new Map<string, string>(),
    getItem = vi.fn((key: string) => entries.get(key) ?? null),
    setItem = vi.fn((key: string, value: string) => {
      entries.set(key, value);
    });
  vi.stubGlobal("sessionStorage", { getItem, setItem });
  return { entries, getItem, setItem };
}
afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});
it("retains exact ASR control intent and protocol locator only under one fixed key and scope", () => {
  const s = storage();
  writeCaptionRecoveryLocator(connection, "project", "recovery");
  writeTranscriptionRequest(connection, original());
  expect(readCaptionRecoveryLocator(connection, "project")).toBe("recovery");
  expect(readTranscriptionRequest(connection, "project", "scene")).toEqual(
    original(),
  );
  expect(
    readTranscriptionRequest(connection, "project", "other-scene"),
  ).toBeNull();
  expect(
    readCaptionRecoveryLocator(
      JSON.stringify(["http://localhost:8788", "other", "credential", false]),
      "project",
    ),
  ).toBeNull();
  expect([...s.entries.keys()]).toEqual(["opencut.caption-client.v1"]);
  expect(JSON.parse(s.entries.get("opencut.caption-client.v1")!)).toMatchObject(
    { schemaVersion: 1 },
  );
});
it("late clear cannot erase a newer request or a different original locator", () => {
  storage();
  writeTranscriptionRequest(connection, original());
  const newer = { ...original(), idempotency_key: "newer" };
  writeTranscriptionRequest(connection, newer);
  clearTranscriptionRequest(connection, original());
  expect(readTranscriptionRequest(connection, "project", "scene")).toEqual(
    newer,
  );
  writeCaptionRecoveryLocator(connection, "project", "recovery");
  clearCaptionRecoveryLocator(connection, "project", "different");
  expect(readCaptionRecoveryLocator(connection, "project")).toBe("recovery");
  expect(() =>
    writeCaptionRecoveryLocator(connection, "project", "different"),
  ).toThrow();
  clearCaptionRecoveryLocator(connection, "project", "recovery");
  clearTranscriptionRequest(connection, newer);
  expect(readCaptionRecoveryLocator(connection, "project")).toBeNull();
  expect(readTranscriptionRequest(connection, "project", "scene")).toBeNull();
});
it("rejects extra content, tokens, media, paths, bad language and noncanonical identity before writing", () => {
  const s = storage();
  for (const extra of ["content", "token", "path", "media"])
    expect(() =>
      writeTranscriptionRequest(connection, {
        ...original(),
        [extra]: "forbidden",
      } as TranscriptionRequestV4),
    ).toThrow();
  expect(() =>
    writeTranscriptionRequest(connection, {
      ...original(),
      revision: "/private/tmp/media",
    }),
  ).toThrow();
  expect(() =>
    writeTranscriptionRequest(connection, { ...original(), language: "EN" }),
  ).toThrow();
  expect(() =>
    writeTranscriptionRequest(" " + connection, original()),
  ).toThrow();
  expect(() =>
    writeCaptionRecoveryLocator(
      JSON.stringify([
        "http://user:secret@localhost",
        "editor",
        "credential",
        false,
      ]),
      "project",
      "recovery",
    ),
  ).toThrow();
  expect(s.setItem).not.toHaveBeenCalled();
});
it("keeps corrupted, scope-mismatched, duplicate or over-UTF8-quota records fail-closed", () => {
  const s = storage(),
    entry = {
      kind: "transcription",
      identity: connection,
      projectId: "project",
      sceneId: "scene",
      request: original(),
    };
  const cases = [
    "{",
    JSON.stringify({ schemaVersion: 2, entries: [] }),
    JSON.stringify({ schemaVersion: 1, entries: [], token: "forbidden" }),
    JSON.stringify({
      schemaVersion: 1,
      entries: [{ ...entry, request: { ...original(), project_id: "other" } }],
    }),
    JSON.stringify({ schemaVersion: 1, entries: [entry, entry] }),
    JSON.stringify({
      schemaVersion: 1,
      entries: [],
      padding: "字".repeat(6000),
    }),
  ];
  for (const raw of cases) {
    s.entries.set("opencut.caption-client.v1", raw);
    expect(() =>
      readTranscriptionRequest(connection, "project", "scene"),
    ).toThrow();
    expect(s.entries.get("opencut.caption-client.v1")).toBe(raw);
  }
});
it("caps combined entry count and requires exact write readback", () => {
  const s = storage();
  for (let i = 0; i < 32; i++)
    writeCaptionRecoveryLocator(connection, `project-${i}`, `recovery-${i}`);
  const full = s.entries.get("opencut.caption-client.v1");
  expect(() => writeTranscriptionRequest(connection, original())).toThrow();
  expect(s.entries.get("opencut.caption-client.v1")).toBe(full);
  s.entries.clear();
  s.setItem.mockImplementationOnce(() => {});
  expect(() => writeTranscriptionRequest(connection, original())).toThrow();
  expect(readTranscriptionRequest(connection, "project", "scene")).toBeNull();
  s.setItem.mockImplementationOnce(() => {
    throw new Error("QuotaExceededError");
  });
  expect(() => writeTranscriptionRequest(connection, original())).toThrow();
});

it("retains the original when a key is reused with changed language and applies UTF8 byte quota", () => {
  const s = storage();
  writeTranscriptionRequest(connection, original());
  expect(() =>
    writeTranscriptionRequest(connection, { ...original(), language: "en" }),
  ).toThrow();
  expect(readTranscriptionRequest(connection, "project", "scene")).toEqual(
    original(),
  );
  s.entries.clear();
  const largeIdentity = JSON.stringify([
    "http://localhost:8788",
    "字".repeat(900),
    "credential",
    false,
  ]);
  let rejected = false;
  for (let i = 0; i < 32; i++) {
    const before = s.entries.get("opencut.caption-client.v1");
    try {
      writeCaptionRecoveryLocator(
        largeIdentity,
        `project-${i}`,
        `recovery-${i}`,
      );
    } catch {
      expect(s.entries.get("opencut.caption-client.v1")).toBe(before);
      expect(i).toBeLessThan(32);
      rejected = true;
      break;
    }
  }
  expect(rejected).toBe(true);
});
