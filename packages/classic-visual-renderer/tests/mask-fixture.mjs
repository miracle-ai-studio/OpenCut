import { blendBeginV1 } from "./blend-fixture.mjs";
export function maskFactV1(kind = "rectangle", feather = 4) {
  return { id: "mask", enabled: true, shape: { kind, geometry: { center_x: 0, center_y: 0, width: 0.6, height: 0.6, rotation_degrees: 0, scale: 1 } }, inverted: false,
    feather: { unit: "classic_pixels", value: feather }, stroke: { width_px: 0, align: "center", color: { encoding: "srgb", red: 0, green: 0, blue: 0, alpha: 1 } } };
}
export function maskBeginV1(time = 0) {
  const packet = blendBeginV1(time);
  packet.type = "beginMaskFrameV1";
  packet.plan = { identity: { ...packet.plan.identity, profile: "classic-v31-gpu-mask-frame-v1" }, execution: packet.plan,
    masks: [{ elementId: "shape", mask: maskFactV1() }, { elementId: "back", mask: null }] };
  packet.admission.maxDecodedPixels = "512";
  return packet;
}
