import { expect, it } from "vitest";
import { previewExecutionAdmission } from "./visual-preview-admission";

it("converts the exact wire allocation budget while preserving other policy facts", () => {
  const wire = { maxEncodedBytes: "8388608", maxFramePixels: "57600", maxRgbaBytes: "16777216", maxDecodedPixels: "230400" };
  expect(previewExecutionAdmission(wire)).toEqual({ ...wire, maxEncodedBytes: 8388608 });
  expect(wire.maxEncodedBytes).toBe("8388608");
  expect(previewExecutionAdmission({ maxEncodedBytes: String(Number.MAX_SAFE_INTEGER) }).maxEncodedBytes).toBe(Number.MAX_SAFE_INTEGER);
});

it.each(["0", "-1", "01", "1.0", "1e6", " 8", "", "9007199254740992", "18446744073709551615"])(
  "rejects an invalid or lossy allocation budget %s before rendering", value => {
    expect(() => previewExecutionAdmission({ maxEncodedBytes: value })).toThrow("Invalid encoded byte admission");
  },
);
