import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { execFileSync } from "node:child_process";
export async function verifySnapshots(page, inspect, directory, check) {
  const before = (await inspect()).project;
  await page.context().grantPermissions(["clipboard-read", "clipboard-write"]);
  await page.evaluate(() => {
    window.showSaveFilePicker = async () => ({
      createWritable: async () => {
        const chunks = [];
        return new WritableStream({
          write(chunk) {
            chunks.push(chunk);
          },
          close() {
            window.savedSnapshotBlob = new Blob(chunks);
          },
        });
      },
    });
  });
  await expect(
    page.getByRole("button", { name: "Save snapshot", exact: true }),
  ).toBeEnabled();
  await page
    .getByRole("button", { name: "Save snapshot", exact: true })
    .click();
  await expect
    .poll(() => page.evaluate(() => window.savedSnapshotBlob?.size ?? 0), {
      timeout: 120000,
    })
    .toBeGreaterThan(0);
  const saved = await page.evaluate(async () =>
    Array.from(new Uint8Array(await window.savedSnapshotBlob.arrayBuffer())),
  );
  const png = path.join(directory, "scene-snapshot.png");
  await fs.writeFile(png, Buffer.from(saved));
  function pixels(file) {
    const info = JSON.parse(
      execFileSync(
        "ffprobe",
        ["-v", "error", "-show_streams", "-of", "json", file],
        { encoding: "utf8" },
      ),
    ).streams[0];
    expect(info.codec_name).toBe("png");
    const sequence = before.base.sequences.find((s) => s.name === "Cutaways");
    expect([info.width, info.height]).toEqual([
      sequence.width,
      sequence.height,
    ]);
    const bytes = execFileSync(
      "ffmpeg",
      [
        "-hide_banner",
        "-loglevel",
        "error",
        "-i",
        file,
        "-frames:v",
        "1",
        "-f",
        "rawvideo",
        "-pix_fmt",
        "rgb24",
        "pipe:1",
      ],
      { maxBuffer: 64 * 1024 * 1024 },
    );
    const at = (x, y) => [
      ...bytes.subarray((y * info.width + x) * 3, (y * info.width + x) * 3 + 3),
    ];
    expect(
      at(Math.floor(info.width / 2), Math.floor(info.height / 2)).every(
        (c) => c >= 248,
      ),
    ).toBe(true);
    expect(at(4, 4).every((c) => c <= 6)).toBe(true);
    return bytes;
  }
  const expected = pixels(png);
  await expect(
    page.getByRole("button", { name: "Copy snapshot", exact: true }),
  ).toBeEnabled();
  await page
    .locator(".preview-stage")
    .click({ button: "right", position: { x: 20, y: 20 } });
  await expect(
    page.getByRole("menu", { name: "Preview actions" }),
  ).toBeVisible();
  await page
    .getByRole("menuitem", { name: "Copy snapshot", exact: true })
    .click();
  await expect(
    page.getByRole("status").filter({ hasText: "Snapshot copied" }),
  ).toBeVisible({ timeout: 120000 });
  const copied = await page.evaluate(async () => {
    const items = await navigator.clipboard.read(),
      item = items.find((item) => item.types.includes("image/png"));
    return Array.from(
      new Uint8Array(await (await item.getType("image/png")).arrayBuffer()),
    );
  });
  const copy = path.join(directory, "copied-snapshot.png");
  await fs.writeFile(copy, Buffer.from(copied));
  expect(pixels(copy).equals(expected)).toBe(true);
  expect((await inspect()).project).toEqual(before);
  await page
    .locator(".preview-stage")
    .click({ button: "right", position: { x: 20, y: 20 } });
  await page.keyboard.press("ArrowDown");
  await expect(
    page.getByRole("menuitem", { name: "Copy snapshot", exact: true }),
  ).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("menu")).toHaveCount(0);
  let submits = 0;
  const request = (req) => {
    if (req.method() === "POST" && req.url().includes("opencut_job_v3_frame"))
      submits++;
  };
  page.on("request", request);
  await page.evaluate(() => {
    window.showSaveFilePicker = async () => {
      throw new DOMException("Cancelled", "AbortError");
    };
  });
  await page
    .getByRole("button", { name: "Save snapshot", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Save snapshot", exact: true }),
  ).toBeEnabled();
  expect(submits).toBe(0);
  page.off("request", request);
  await expect(page.getByRole("alert")).toHaveCount(0);
  check(
    "Classic snapshot save and real image clipboard use the selected scene at full canvas resolution; independent PNG pixels agree, menus support keyboard and cancelled saving submits no job or project edit",
  );
}
