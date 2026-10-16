import { expect, it } from "vitest";
import { settingsPatch } from "./settings";
import { initialSnapshot } from "../fixture";
it("custom canvas stores last size while preserving background, original dimensions and exact FPS ratio", () => {
  const source = initialSnapshot().project.settings;
  if (source.profile !== "classic_global") throw Error("classic");
  source.settings.original_canvas_size = { width: 1440, height: 1080 };
  const next = settingsPatch(
    source,
    "scene",
    "1080",
    "1920",
    "30000",
    "1001",
    "custom",
  );
  if (next.profile !== "classic_global") throw Error("classic");
  expect(next.settings.canvas_size).toEqual({ width: 1080, height: 1920 });
  expect(next.settings.last_custom_canvas_size).toEqual(
    next.settings.canvas_size,
  );
  expect(next.settings.original_canvas_size).toEqual({
    width: 1440,
    height: 1080,
  });
  expect(next.settings.fps).toEqual({ numerator: 30000, denominator: 1001 });
  expect(next.settings.background).toEqual(source.settings.background);
  expect(source.settings.canvas_size).toEqual({ width: 640, height: 360 });
});
it("preset changes preserve the previous custom size; zero, fractional or overflowing dimensions fail before submitting", () => {
  const source = initialSnapshot().project.settings;
  if (source.profile !== "classic_global") throw Error("classic");
  source.settings.last_custom_canvas_size = { width: 1234, height: 567 };
  const next = settingsPatch(
    source,
    "scene",
    "1920",
    "1080",
    "24",
    "1",
    "preset",
  );
  if (next.profile !== "classic_global") throw Error("classic");
  expect(next.settings.last_custom_canvas_size).toEqual({
    width: 1234,
    height: 567,
  });
  expect(next.settings.canvas_size_mode).toBe("preset");
  for (const width of ["0", "-1", "1.5", "4294967296", ""])
    expect(() =>
      settingsPatch(source, "scene", width, "1080", "24", "1", "custom"),
    ).toThrow();
});
