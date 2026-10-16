import { createHash } from "node:crypto";
export const blendBytes = Uint8Array.of(1, 2, 3, 4);
export function blendBeginV1(frameIndex = 0) {
  const fixed = {
    projectId: "p",
    revision: "r",
    sequenceId: "s",
    time: { ticks: String(frameIndex), timebase: 30 },
    width: 8,
    height: 4,
  };
  const resource = {
    assetId: "media",
    kind: "image",
    size: "4",
    contentHash: createHash("sha256").update(blendBytes).digest("hex"),
  };
  const text = {
    identity: { ...fixed, profile: "classic-v31-gpu-normal-text-frame-v2" },
    media: {
      identity: { ...fixed, profile: "classic-v31-gpu-normal-frame" },
      resources: [resource],
      layers: [{ elementId: "back", resource, decode: { kind: "image" } }],
    },
    texts: [],
    order: ["back"],
  };
  const graphic = {
    identity: { ...fixed, profile: "classic-v31-gpu-normal-graphic-frame-v1" },
    execution: {
      identity: {
        ...fixed,
        profile: "classic-v31-gpu-normal-caption-frame-v3",
      },
      execution: text,
      captions: [],
    },
    graphics: [
      {
        elementId: "shape",
        graphic: {
          sourceWidth: 2,
          sourceHeight: 2,
          geometry: { kind: "star", points: 5, depthPercent: 45 },
          fill: [1, 0, 0, 0.5],
          stroke: { color: [0, 1, 0, 1], widthPx: 0, align: "inside" },
        },
        quad: {
          centerX: 5,
          centerY: 1,
          width: 4,
          height: 4,
          rotationRadians: 1,
          opacity: 0.5,
          flipX: true,
          flipY: false,
        },
      },
    ],
    order: ["shape", "back"],
  };
  return {
    protocolVersion: 1,
    type: "beginBlendFrameV1",
    executionId: "blend",
    frameIndex,
    plan: {
      identity: { ...fixed, profile: "classic-v31-gpu-blend-frame-v1" },
      execution: graphic,
      blends: [
        { elementId: "shape", blendMode: "multiply" },
        { elementId: "back", blendMode: "hue" },
      ],
    },
    resources: [{ assetId: "media", handle: "private-media" }],
    fonts: [],
    admission: {
      maxEncodedBytes: 32,
      maxFramePixels: "32",
      maxDecodedPixels: "128",
      maxRgbaBytes: "4096",
      maxGpuBytes: "4096",
      maxOutputBytes: "1048576",
      maxRssBytes: "536870912",
      deadlineMs: 1000,
      quotaProfile: "monitored-terminate",
    },
    runtime: { chromiumExecutable: "/host/browser" },
  };
}
export const graphicObservations = {
  graphics: [
    { elementId: "shape", width: 2, height: 2, originX: 0, originY: 0 },
  ],
};
