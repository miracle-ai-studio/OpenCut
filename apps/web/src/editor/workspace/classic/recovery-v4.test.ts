import { expect, it, vi } from "vitest";
import { OpenCutClient } from "../../../lib/editor-sdk";
import { prepareRecovery } from "./recovery";
import {
  prepareRestoration,
  reviewRestoration,
  mapRecoveryFonts,
  publishRestoration,
  restorationRequest,
  findRestoration,
  matchesRecoveryFont,
} from "./restore";
import type { ClassicSnapshot } from "./decode";
const signal = () => new AbortController().signal;
const snapshot = (): ClassicSnapshot => ({
  origin: "https://old.example",
  sourceId: "classic-source",
  media: [],
  context: { legacyTracksBySceneId: {}, mediaTypesById: {} },
  raw: {
    id: "classic-source",
    metadata: { id: "classic-source", name: "Original" },
    version: 31,
    settings: {
      fps: { numerator: 30, denominator: 1 },
      canvasSize: { width: 320, height: 180 },
      background: { type: "color", color: "#102030" },
    },
    scenes: [
      {
        id: "scene",
        tracks: {
          main: { id: "main", type: "video", elements: [] },
          overlay: [
            {
              id: "text-track",
              type: "text",
              elements: [
                {
                  id: "text",
                  type: "text",
                  content: "OpenCut\n中文",
                  fontSize: 12,
                  fontFamily: "Lato",
                  sourceDuration: null,
                  trimStart: 0.2,
                  trimEnd: 0.3,
                  animation: { position_x: [{ time: 0, value: 12 }] },
                },
              ],
            },
          ],
          audio: [],
        },
        bookmarks: [],
      },
    ],
  },
});
const analysis = {
  source_content_id: "source-hash",
  source_format: "classic",
  source_version: 31,
  target_project_id: "target",
  mapped: [],
  blockers: [],
  losses: [],
  unresolved_fonts: [
    { family: "Lato", font_weight: "normal", font_style: "normal" },
  ],
  proposed_project: { id: "restored" },
};
async function setup() {
  const prepared = await prepareRestoration(snapshot(), [], signal(), () => {});
  const stage = {
    revision: "stage-fixed",
    project: { id: prepared.stageId, name: "Preparing", assets: [] },
  };
  const sdk = new OpenCutClient({ baseUrl: "http://localhost", token: "" });
  const call = vi.spyOn(sdk, "call").mockImplementation(async (name) => {
    if (name === "opencut_project_create")
      return {project_id: stage.project.id, revision: stage.revision} as never;
    if (name === "opencut_project_inspect")
      return {kind: "editor", snapshot: stage} as never;
    if (name === "opencut_classic_analyze")
      return analysis as never;
    throw new Error(`Unexpected call ${name}`);
  });
  const review = await reviewRestoration(sdk, prepared, signal(), () => {});
  return { sdk, call, prepared, review };
}
it("preserves complete v31 source and separates both IDs from legacy recovery", async () => {
  const source = snapshot(),
    original = structuredClone(source);
  const legacy = await prepareRecovery(source, [], signal(), () => {});
  const current = await prepareRestoration(source, [], signal(), () => {});
  expect(current.targetId).not.toBe(legacy.targetId);
  expect(current.stageId).not.toBe(legacy.stageId);
  const expected = structuredClone(source.raw);
  delete expected.id;
  expect(JSON.parse(current.decoded.content)).toEqual(expected);
  expect(source).toEqual(original);
});
it("shares staging and calls one canonical analysis without a protocol selector",async()=>{
  const {call,review}=await setup();
  expect(call.mock.calls.filter(([name])=>name==="opencut_classic_analyze")).toHaveLength(1);
  expect(review).not.toHaveProperty("protocol");
  expect(review.analysis).not.toHaveProperty("schema_version");
});
it("font reanalysis retains source/media revision and changes the request key", async () => {
  const { sdk, review } = await setup();
  const original = structuredClone(review.source);
  const mappings = [
    {
      classic_family: "Lato",
      font_weight: "normal" as const,
      font_style: "normal" as const,
      source_project_id: "fonts",
      source_revision: "font-fixed",
      font_id: "lato400",
    },
  ];
  const mapped = await mapRecoveryFonts(sdk, review, mappings, signal());
  mappings[0].source_revision = "changed-head";
  expect(mapped.source).toEqual({
    ...original,
    font_mappings: [{ ...mappings[0], source_revision: "font-fixed" }],
  });
  expect(review.source).toEqual(original);
  expect(restorationRequest(mapped).idempotency_key).not.toBe(
    restorationRequest(review).idempotency_key,
  );
});
it("unknown acknowledgement retries the same request without promotion", async () => {
  const { sdk, review, call } = await setup();
  const mutation = vi
    .spyOn(sdk, "callWithAdmissionRetry")
    .mockRejectedValueOnce(new Error("acknowledgement lost"))
    .mockResolvedValueOnce({
      target_project_id: review.prepared.targetId,
    } as never);
  await expect(publishRestoration(sdk, review, signal())).rejects.toThrow(
    "acknowledgement lost",
  );
  expect(await publishRestoration(sdk, review, signal())).toBe(
    review.prepared.targetId,
  );
  expect(mutation.mock.calls[0].slice(0, 2)).toEqual(
    mutation.mock.calls[1].slice(0, 2),
  );
  expect(mutation.mock.calls[0][0]).toBe("opencut_classic_import");
  expect(call.mock.calls.map(([name]) => name)).not.toContain(
    "opencut_project_promote",
  );
});
it("does not publish blocked analyses", async () => {
  const { sdk, review } = await setup();
  const mutation = vi.spyOn(sdk, "callWithAdmissionRetry");
  await expect(
    publishRestoration(
      sdk,
      {
        ...review,
        analysis: {
          ...review.analysis,
          blockers: [
            {
              path: "$.text",
              code: "UNSUPPORTED",
              detail: "Preserve original",
            },
          ],
        },
      },
      signal(),
    ),
  ).rejects.toThrow("unsupported content");
  expect(mutation).not.toHaveBeenCalled();
});
it("checks immutable import revision and rejects mismatched source receipt", async () => {
  const { sdk, prepared } = await setup();
  const call = vi.spyOn(sdk, "call").mockImplementation(async (name, input) => {
    if (
      name === "opencut_event_changes" &&
      !(input as { cursor: string | null }).cursor
    )
      return {
        events: Array.from({ length: 100 }, () => ({
          event: { type: "project", data: { event_type: "project.edited" } },
        })),
        next_cursor: "page-two",
      } as never;
    if (name === "opencut_event_changes")
      return {
        events: [
          {
            event: {
              type: "project",
              data: {
                event_type: "project.classic_imported_v4",
                revision: "original-import",
                payload: { source_content_id: "other-source" },
              },
            },
          },
        ],
      } as never;
    if (name === "opencut_project_inspect")
      return {kind: "editor", snapshot: { revision: "original-import", project: { assets: [] } }} as never;
    if (name === "opencut_classic_analyze") return analysis as never;
    throw new Error(`Unexpected call ${name}`);
  });
  call.mockClear();
  await expect(findRestoration(sdk, prepared, signal())).rejects.toThrow(
    "no project will be overwritten",
  );
  expect(
    call.mock.calls.find(
      ([name]) => name === "opencut_project_inspect",
    )?.[1],
  ).toEqual({ project_id: prepared.targetId, revision: "original-import" });
  expect(
    call.mock.calls.filter(([name]) => name === "opencut_event_changes"),
  ).toHaveLength(2);
});

it("stops at an empty event tail even when its continuation cursor is retained", async () => {
  const { sdk, prepared } = await setup();
  const call = vi
    .spyOn(sdk, "call")
    .mockImplementation(async (_name, input) => {
      if (!(input as { cursor: string | null }).cursor)
        return {
          events: [
            {
              event: {
                type: "project",
                data: { event_type: "project.edited" },
              },
            },
          ],
          next_cursor: "tail",
        } as never;
      return { events: [], next_cursor: "tail" } as never;
    });
  call.mockClear();
  expect(await findRestoration(sdk, prepared, signal())).toBeNull();
  expect(call).toHaveBeenCalledTimes(2);
});

it("retries one exact canonical publication after a lost acknowledgment",async()=>{
  const {sdk,prepared,review}=await setup();
  const mutation=vi.spyOn(sdk,"callWithAdmissionRetry")
    .mockRejectedValueOnce(new Error("acknowledgement lost"))
    .mockResolvedValueOnce({target_project_id:prepared.targetId} as never);
  await expect(publishRestoration(sdk,review,signal())).rejects.toThrow("acknowledgement lost");
  await expect(publishRestoration(sdk,review,signal())).resolves.toBe(prepared.targetId);
  expect(mutation.mock.calls[0][0]).toBe("opencut_classic_import");
  expect(mutation.mock.calls[0].slice(0,2)).toEqual(mutation.mock.calls[1].slice(0,2));
});

it("offers exact catalog aliases and explicit requested italic while retaining native metadata", () => {
  const font = { id: "font", family: "Native family", weight: 700, style: "normal", content: { content_hash: "a".repeat(64), size: 1234 } } as never;
  const request = { family: "Classic family", font_weight: "bold", font_style: "italic" } as const;
  const face = { display_family: "Classic family", family: "Native family", weight: 700, style: "normal", requested_style: "italic", content_hash: "a".repeat(64), size_bytes: "1234" } as Parameters<typeof matchesRecoveryFont>[2][number];
  expect(matchesRecoveryFont(font, request, [face])).toBe(true);
  expect(matchesRecoveryFont(font, request, [])).toBe(false);
  for (const change of [{ family: "Other" }, { weight: 400 }, { style: "italic" }, { content_hash: "b".repeat(64) }, { size_bytes: "1235" }])
    expect(matchesRecoveryFont(font, request, [{ ...face, ...change } as never])).toBe(false);
});
