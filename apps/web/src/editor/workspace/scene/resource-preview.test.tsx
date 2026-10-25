// @vitest-environment jsdom
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { webcrypto } from "node:crypto";
import { CatalogSoundPreview } from "./catalog-sound-preview";
import type { CatalogEntry } from "./catalog_resources";
import { preferences } from "../../shared/locale";
const mocks = vi.hoisted(() => ({
  api: { connectionIdentity: "local-owner", sdk: { call: vi.fn() } },
  context: null as null | { begin: ReturnType<typeof vi.fn>; release: ReturnType<typeof vi.fn> },
}));
vi.mock("../context", () => ({ useWorkspace: () => ({ api: mocks.api }) }));
vi.mock("./workbench/player-context", () => ({ usePlayerContext: () => mocks.context }));
const hash = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
const entry: CatalogEntry = {
  id: "kenney-ui-audio:click1",
  name: "click1",
  category: "sounds",
  source_sha256: hash,
  audio: null,
  keywords: [],
  source_url: "https://kenney.nl/assets/ui-audio",
  width: null,
  height: null,
  license: {
    identifier: "CC0-1.0",
    source_url: null,
    notice: null,
    attribution: null,
  },
};
const response = () => ({
  catalog_id: entry.id,
  version: { id: `av_${hash}`, content_hash: hash, size: 3 },
  mime_type: "audio/ogg",
  bytes_hex: "616263",
});
beforeEach(() => {
  mocks.context = null;
  preferences({ language: "en" });
  mocks.api.sdk.call.mockReset().mockResolvedValue(response());
  vi.stubGlobal("crypto", webcrypto);
  vi.stubGlobal(
    "URL",
    Object.assign(URL, {
      createObjectURL: vi.fn(() => "blob:audition"),
      revokeObjectURL: vi.fn(),
    }),
  );
  vi.spyOn(HTMLMediaElement.prototype, "play").mockResolvedValue();
  vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => {});
  vi.stubGlobal("requestAnimationFrame", vi.fn(() => 1));
  vi.stubGlobal("cancelAnimationFrame", vi.fn());
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});
it("auditions only verified bytes through the read-only endpoint and releases them on leaving", async () => {
  const view = render(<CatalogSoundPreview entry={entry} onClose={() => {}} />);
  await screen.findByRole("button", { name: /^Listen/ });
  expect(mocks.api.sdk.call).toHaveBeenCalledExactlyOnceWith(
    "opencut_catalog_resources_preview",
    { catalog_id: entry.id },
    { signal: expect.any(AbortSignal) },
  );
  const audio = view.container.querySelector("audio")!;
  fireEvent.canPlay(audio);
  expect(audio.play).toHaveBeenCalledOnce();
  fireEvent.canPlay(audio);
  expect(audio.play).toHaveBeenCalledOnce();
  view.unmount();
  expect(audio.pause).toHaveBeenCalled();
  expect(URL.revokeObjectURL).toHaveBeenCalledWith("blob:audition");
});
it.each(["id", "size", "hash", "bytes", "oversize", "mime"])(
  "refuses an invalid %s before creating a playable URL",
  async (kind) => {
    const value = response();
    if (kind === "id") value.catalog_id = "other";
    if (kind === "size") value.version.size = 4;
    if (kind === "hash") value.version.content_hash = "a".repeat(64);
    if (kind === "bytes") value.bytes_hex = "616264";
    if (kind === "oversize") value.version.size = 262145;
    if (kind === "mime") value.mime_type = "text/html";
    mocks.api.sdk.call.mockResolvedValue(value);
    render(<CatalogSoundPreview entry={entry} onClose={() => {}} />);
    await screen.findByRole("alert");
    expect(URL.createObjectURL).not.toHaveBeenCalled();
  },
);
it("ignores a late read after the selected sound is closed", async () => {
  let resolve!: (value: ReturnType<typeof response>) => void;
  mocks.api.sdk.call.mockReturnValue(
    new Promise((done) => {
      resolve = done;
    }),
  );
  const view = render(<CatalogSoundPreview entry={entry} onClose={() => {}} />);
  const signal = mocks.api.sdk.call.mock.calls[0][2].signal;
  view.unmount();
  expect(signal.aborted).toBe(true);
  await act(async () => {
    resolve(response());
  });
  expect(URL.createObjectURL).not.toHaveBeenCalled();
});
it("keeps inactive rows read-only and starts loading only after explicit activation", async () => {
  const activate = vi.fn();
  render(<CatalogSoundPreview entry={entry} active={false} onActivate={activate} onClose={() => {}} />);
  expect(mocks.api.sdk.call).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: /^Listen/ }));
  expect(activate).toHaveBeenCalledOnce();
  expect(mocks.api.sdk.call).not.toHaveBeenCalled();
});
it("uses actual media events for play/pause and progress, without a second play or close action", async () => {
  const view = render(<CatalogSoundPreview entry={entry} onClose={() => {}} />);
  const button = await screen.findByRole("button", { name: /^Listen/ });
  const audio = view.container.querySelector("audio")!;
  Object.defineProperty(audio, "duration", { configurable: true, value: 4 });
  Object.defineProperty(audio, "currentTime", { configurable: true, writable: true, value: 1 });
  fireEvent.loadedMetadata(audio);
  fireEvent.timeUpdate(audio);
  expect(button.getAttribute("aria-pressed")).toBe("false");
  expect(screen.getByRole("progressbar").getAttribute("aria-valuenow")).toBe("25");
  fireEvent.play(audio);
  expect(screen.getAllByRole("button")).toHaveLength(1);
  expect(screen.getByRole("button", { name: /^Pause audition/ }).getAttribute("aria-pressed")).toBe("true");
  fireEvent.pause(audio);
  expect(screen.getByRole("button", { name: /^Listen/ }).getAttribute("aria-pressed")).toBe("false");
  expect(screen.getByRole("progressbar").getAttribute("aria-valuenow")).toBe("25");
});
it("retries the same source read after a failure without importing or editing", async () => {
  mocks.api.sdk.call.mockRejectedValueOnce(Error("offline"));
  render(<CatalogSoundPreview entry={entry} onClose={() => {}} />);
  await screen.findByRole("alert");
  fireEvent.click(screen.getByRole("button", { name: /^Retry audition/ }));
  await screen.findByRole("button", { name: /^Listen/ });
  expect(mocks.api.sdk.call).toHaveBeenCalledTimes(2);
  for (const [operation, request] of mocks.api.sdk.call.mock.calls) {
    expect(operation).toBe("opencut_catalog_resources_preview");
    expect(request).toEqual({ catalog_id: entry.id });
  }
});
it("never starts late audio after a pending Native pause outlives the audition", async () => {
  let confirm!: (value: boolean) => void;
  const begin = vi.fn(() => new Promise<boolean>(resolve => { confirm = resolve; }));
  const release = vi.fn(); mocks.context = { begin, release };
  const view = render(<CatalogSoundPreview entry={entry} onClose={() => {}} />);
  await screen.findByRole("button", { name: /^Listen/ });
  const audio = view.container.querySelector("audio")!;
  fireEvent.canPlay(audio);
  expect(begin).toHaveBeenCalledOnce();
  expect(audio.play).not.toHaveBeenCalled();
  view.unmount();
  await act(async () => confirm(true));
  expect(audio.play).not.toHaveBeenCalled();
  expect(release).toHaveBeenCalled();
});

const analyzed = (): CatalogEntry => ({ ...entry, audio: {
  source_sha256: hash, source_size: 3, producer_sha256: "a".repeat(64),
  identity: { algorithm: "source-peaks128-v1", decode_profile: "fixed-catalog-webaudio-decode-source-peaks", browser_version: "test", browser_sha256: "a".repeat(64), node_sha256: "b".repeat(64), module_sha256: "c".repeat(64) },
  audition_duration_seconds: 4, sample_rate: 48000, channels: 2,
  total_samples: "256", bucket_size: 128, amplitudes: [0, .5],
} });
it("renders a genuine silent bucket without noise and keeps the source overview separate from media progress", () => {
  const view = render(<CatalogSoundPreview entry={analyzed()} active={false} onClose={() => {}} />);
  expect(view.container.querySelector(".resource-audio-overview svg path")?.getAttribute("d")).toBe("M0.5,14v0M1.5,8v12");
  expect(view.container.querySelector(".resource-audio-duration")?.textContent).toBe("4.00s");
  expect(screen.getByRole("progressbar").getAttribute("aria-valuenow")).toBe("0");
  expect(mocks.api.sdk.call).not.toHaveBeenCalled();
});
it("rejects analysis bound to a different source size before making the encoded sound playable", async () => {
  const value = analyzed(); value.audio!.source_size = 4;
  render(<CatalogSoundPreview entry={value} onClose={() => {}} />);
  await screen.findByRole("alert");
  expect(URL.createObjectURL).not.toHaveBeenCalled();
});
