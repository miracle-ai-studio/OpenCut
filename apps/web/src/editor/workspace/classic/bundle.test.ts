import { expect, it } from "vitest";
import {
  createRecoveryBundle,
  readRecoveryBundle,
  bundleMagic,
} from "./bundle";
import { fileDigest } from "./identity";
import type { ClassicSnapshot } from "./decode";
const signal = () => new AbortController().signal;
async function fixture() {
  const file = new File(
    [new Uint8Array([0, 255, 13, 10, 42])],
    "original.wav",
    { type: "audio/wav" },
  );
  const snapshot: ClassicSnapshot = {
    origin: "https://classic.example",
    sourceId: "old",
    raw: { id: "old", version: 1 },
    context: {
      legacyTracksBySceneId: { main: [{ id: "track", elements: [] }] },
      mediaTypesById: { media: "audio" },
    },
    media: [{ id: "media", name: file.name, size: file.size, type: "audio" }],
  };
  const media = [
    { id: "media", file, sha256: await fileDigest(file, signal()) },
  ];
  const bundle = new File(
    [await createRecoveryBundle(snapshot, media, signal())],
    "project.occlassic",
  );
  return { snapshot, media, bundle };
}
async function changeHeader(
  file: File,
  change: (header: any) => void,
  tail?: Uint8Array,
) {
  const length = new DataView(
    await file.slice(bundleMagic.length, bundleMagic.length + 4).arrayBuffer(),
  ).getUint32(0, true);
  const header = JSON.parse(
    await file
      .slice(bundleMagic.length + 4, bundleMagic.length + 4 + length)
      .text(),
  );
  change(header);
  const json = new TextEncoder().encode(JSON.stringify(header)),
    size = new Uint8Array(4);
  new DataView(size.buffer).setUint32(0, json.length, true);
  return new File(
    [
      bundleMagic,
      size,
      json,
      tail ? new Uint8Array(tail) : file.slice(bundleMagic.length + 4 + length),
    ],
    file.name,
  );
}
it("round trips original snapshot, timeline context and exact binary bytes without mutating originals", async () => {
  const { snapshot, media, bundle } = await fixture(),
    original = structuredClone(snapshot);
  const result = await readRecoveryBundle(bundle, signal(), () => {});
  expect(result.snapshot).toEqual(original);
  expect(snapshot).toEqual(original);
  expect(result.media[0].file.name).toBe(media[0].file.name);
  expect(new Uint8Array(await result.media[0].file.arrayBuffer())).toEqual(
    new Uint8Array([0, 255, 13, 10, 42]),
  );
  expect(result.media[0].sha256).toBe(media[0].sha256);
  const historical = await changeHeader(bundle, h => { h.format = "opencut.classic-recovery.v1"; });
  const recovered = await readRecoveryBundle(historical, signal(), () => {});
  expect(recovered.snapshot).toEqual(original);
  expect(recovered.media[0].sha256).toBe(media[0].sha256);
});
it("rejects JSON-only input and missing media before any recovery", async () => {
  const { snapshot } = await fixture();
  await expect(
    readRecoveryBundle(
      new File([JSON.stringify(snapshot)], "project.json"),
      signal(),
      () => {},
    ),
  ).rejects.toThrow("standalone project JSON");
  await expect(createRecoveryBundle(snapshot, [], signal())).rejects.toThrow(
    "every original",
  );
});
it.each([
  [
    "unknown format",
    (h: any) => {
      h.format = "opencut.classic-recovery.v2";
    },
    "format",
  ],
  [
    "unknown header",
    (h: any) => {
      h.extra = true;
    },
    "format",
  ],
  [
    "wrong identity",
    (h: any) => {
      h.snapshot.sourceId = "other";
    },
    "snapshot",
  ],
  [
    "non-origin URL",
    (h: any) => {
      h.snapshot.origin += "/projects";
    },
    "origin",
  ],
  [
    "missing file",
    (h: any) => {
      h.media = [];
    },
    "Incomplete",
  ],
  [
    "duplicate file",
    (h: any) => {
      h.snapshot.media.push({ ...h.snapshot.media[0], id: "another" });
      h.media.push(h.media[0]);
    },
    "entry",
  ],
  [
    "size mismatch",
    (h: any) => {
      h.snapshot.media[0].size++;
    },
    "metadata",
  ],
  [
    "name mismatch",
    (h: any) => {
      h.media[0].name = "other.wav";
    },
    "metadata",
  ],
  [
    "unreviewed file fields",
    (h: any) => {
      h.media[0].path = "/private/input";
    },
    "entry",
  ],
] as const)("rejects %s", async (_name, change, message) => {
  const { bundle } = await fixture();
  await expect(
    readRecoveryBundle(await changeHeader(bundle, change), signal(), () => {}),
  ).rejects.toThrow(message);
});
it("rejects changed bytes, truncated files and trailing data", async () => {
  const { bundle } = await fixture();
  await expect(
    readRecoveryBundle(
      await changeHeader(
        bundle,
        () => {},
        new Uint8Array([0, 255, 13, 10, 43]),
      ),
      signal(),
      () => {},
    ),
  ).rejects.toThrow("checksum mismatch");
  await expect(
    readRecoveryBundle(
      new File([bundle.slice(0, bundle.size - 1)], bundle.name),
      signal(),
      () => {},
    ),
  ).rejects.toThrow("entry");
  await expect(
    readRecoveryBundle(
      new File([bundle, "x"], bundle.name),
      signal(),
      () => {},
    ),
  ).rejects.toThrow("trailing bytes");
});
it("cancellation and nonfinite source values preserve the original input", async () => {
  const { bundle, snapshot, media } = await fixture(),
    abort = new AbortController();
  abort.abort();
  await expect(
    readRecoveryBundle(bundle, abort.signal, () => {}),
  ).rejects.toThrow();
  snapshot.raw.fps = Infinity;
  await expect(createRecoveryBundle(snapshot, media, signal())).rejects.toThrow(
    "Nonfinite",
  );
  expect(snapshot.raw.fps).toBe(Infinity);
  await expect(
    readRecoveryBundle(bundle, signal(), () => {}),
  ).resolves.toHaveProperty("media");
});
