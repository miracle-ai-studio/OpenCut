import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { execFileSync } from "node:child_process";
export async function verifyProjectCovers(browser, endpoint, directory, check) {
  const page = await browser.newPage({
    locale: "en-US",
    viewport: { width: 1440, height: 1000 },
  });
  page.setDefaultTimeout(60000);
  const calls = [],
    errors = [],
    frames = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("request", (r) => {
    if (r.url().endsWith("/api/tools/opencut_job_frame"))
      frames.push(r.postDataJSON());
  });
  let projectId;
  const tool = async (name, input) => {
    const reply = await page.evaluate(
      async ({ name, input }) =>
        (
          await fetch(`/api/tools/${name}`, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(input),
          })
        ).json(),
      { name, input },
    );
    calls.push({ name, input, reply });
    expect(reply.error).toBeNull();
    return reply.data;
  };
  const inspect = (id) =>
    tool("opencut_project_inspect", { project_id: id ?? projectId });
  const card = (name) =>
    page.locator(".project-card-wrap").filter({
      has: page
        .locator(".project-card > strong")
        .getByText(name, { exact: true }),
    });
  const pixels = async (name, color, filename, revision) => {
    const image = card(name).locator(
      ".project-cover img" +
        (revision ? `[data-cover-revision="${revision}"]` : ""),
    );
    await expect(image).toBeVisible({ timeout: 180000 });
    await expect
      .poll(() => image.evaluate((i) => i.complete && i.naturalWidth > 0))
      .toBe(true);
    const info = await image.evaluate(async (i) => ({
      width: i.naturalWidth,
      height: i.naturalHeight,
      bytes: Array.from(
        new Uint8Array(await (await fetch(i.src)).arrayBuffer()),
      ),
    }));
    expect(info.width).toBeLessThanOrEqual(320);
    expect(info.height).toBeLessThanOrEqual(180);
    const file = path.join(directory, filename);
    await fs.writeFile(file, Buffer.from(info.bytes));
    const rgb = execFileSync("ffmpeg", [
      "-v",
      "error",
      "-i",
      file,
      "-frames:v",
      "1",
      "-vf",
      "scale=1:1",
      "-pix_fmt",
      "rgb24",
      "-f",
      "rawvideo",
      "-",
    ]);
    expect(rgb[color === "red" ? 0 : 2]).toBeGreaterThan(180);
    expect(rgb[1]).toBeLessThan(40);
    expect(rgb[color === "red" ? 2 : 0]).toBeLessThan(40);
    return { width: info.width, height: info.height, rgb: [...rgb] };
  };
  const name = "F05 native project cover",
    results = {};
  const back = () =>
    page.getByRole("button", { name: "Back to projects", exact: true }).click();
  const refresh = async (n) => {
    await card(n).locator("summary").click();
    await card(n)
      .getByRole("button", { name: "Refresh cover", exact: true })
      .click();
  };
  try {
    for (const color of ["red", "blue"])
      execFileSync("ffmpeg", [
        "-v",
        "error",
        "-f",
        "lavfi",
        "-i",
        `color=${color}:s=160x90:r=24:d=2`,
        "-c:v",
        "libx264",
        "-pix_fmt",
        "yuv420p",
        path.join(directory, `cover-${color}.mp4`),
      ]);
    await page.goto(`${endpoint}/editor`);
    await page
      .getByRole("button", { name: "New project", exact: true })
      .click();
    await page.getByLabel("Project name").fill(name);
    await page
      .getByRole("button", { name: "Create project", exact: true })
      .click();
    await page.waitForURL(/\/editor\/[^/]+/);
    projectId = new URL(page.url()).pathname.split("/")[2];
    const guide = page.locator(".editor-guide");
    if (await guide.count()) {
      await guide.getByRole("button", { name: "Next", exact: true }).click();
      await guide.getByRole("button", { name: "Next", exact: true }).click();
      await guide.getByRole("button", { name: "Finish", exact: true }).click();
    }
    if (
      await page
        .getByRole("button", { name: "Dismiss update", exact: true })
        .count()
    )
      await page
        .getByRole("button", { name: "Dismiss update", exact: true })
        .click();
    const empty = await inspect();
    await back();
    await expect(card(name).locator(".project-cover img")).toHaveCount(0);
    expect((await inspect()).revision).toBe(empty.revision);
    check(
      "F05: empty new project uses a safe placeholder without changing its content revision",
    );
    await card(name).locator(".project-card").click();
    const media = page.getByRole("region", { name: "Media", exact: true });
    await media
      .locator("input[type=file]")
      .setInputFiles(
        ["red", "blue"].map((c) => path.join(directory, `cover-${c}.mp4`)),
      );
    for (const c of ["red", "blue"])
      await expect(
        media.getByRole("button", {
          name: `Add cover-${c}.mp4 to timeline`,
          exact: true,
        }),
      ).toBeVisible({ timeout: 120000 });
    await media
      .getByRole("button", {
        name: "Add cover-red.mp4 to timeline",
        exact: true,
      })
      .click();
    await expect(
      page.locator(".timeline-clip").filter({ hasText: "cover-red.mp4" }),
    ).toBeVisible();
    const red = await inspect(),
      main = red.project.base.default_sequence_id;
    await fs.unlink(path.join(directory, "cover-red.mp4"));
    await fs.unlink(path.join(directory, "cover-blue.mp4"));
    await back();
    results.red = await pixels(name, "red", "project-cover-red.png");
    expect((await inspect()).revision).toBe(red.revision);
    expect(
      frames.some(
        (f) =>
          f.project_id === projectId &&
          f.revision === red.revision &&
          f.sequence_id === main &&
          f.requested_time.ticks === "0",
      ),
    ).toBe(true);
    const selectedFrames = () =>
      frames.filter(
        (f) =>
          f.project_id === projectId &&
          f.revision === red.revision &&
          f.sequence_id === main,
      ).length;
    const count = selectedFrames();
    await page.getByRole("button", { name: "List view", exact: true }).click();
    await pixels(name, "red", "project-cover-list.png");
    await page.getByRole("button", { name: "Grid view", exact: true }).click();
    await card(name).locator(".project-card").click();
    await back();
    await pixels(name, "red", "project-cover-cached.png");
    expect(selectedFrames()).toBe(count);
    check(
      "F05: managed native main-scene first frame produces independently red thumbnail pixels; grid/list and reopening reuse the fixed-revision cache without content writes",
    );
    const old = red.project.base.assets.find((a) => a.name === "cover-red.mp4"),
      blue = red.project.base.assets.find((a) => a.name === "cover-blue.mp4");
    const changed = await tool("opencut_edit_apply", {
      project_id: projectId,
      expected_revision: red.revision,
      idempotency_key: "f05-blue",
      label: "Replace red with blue",
      operations: [
        {
          type: "base.apply",
          operation: { type: "asset.replace", asset: { ...blue, id: old.id } },
        },
      ],
    });
    await page
      .getByRole("button", { name: "Refresh projects", exact: true })
      .click();
    results.blue = await pixels(
      name,
      "blue",
      "project-cover-blue.png",
      changed.revision,
    );
    expect(
      frames.some(
        (f) => f.project_id === projectId && f.revision === changed.revision,
      ),
    ).toBe(true);
    await page.reload();
    await pixels(name, "blue", "project-cover-reloaded.png");
    check(
      "F05: a new native content revision invalidates the red cover and displays independently blue pixels; reload renders the current revision",
    );
    await card(name).locator(".project-card").click();
    await page
      .getByRole("button", { name: "Manage scenes", exact: true })
      .click();
    const scenes = page.getByRole("dialog", { name: "Scenes", exact: true });
    await scenes
      .getByRole("button", { name: "New scene", exact: true })
      .click();
    await scenes.getByLabel("Scene name", { exact: true }).fill("Other scene");
    await scenes
      .getByRole("button", { name: "Create scene", exact: true })
      .click();
    await expect(scenes.getByLabel("Scene name", { exact: true })).toHaveCount(
      0,
    );
    const other = await page
      .getByLabel("Active sequence", { exact: true })
      .inputValue();
    expect(other).not.toBe(main);
    await scenes.getByRole("button", { name: "Close", exact: true }).click();
    const withScene = await inspect();
    const otherSequence = withScene.project.base.sequences.find(
      (s) => s.id === other,
    );
    const otherRed = await tool("opencut_edit_apply", {
      project_id: projectId,
      expected_revision: withScene.revision,
      idempotency_key: "f05-other-red",
      label: "Red active scene",
      operations: [
        {
          type: "base.apply",
          operation: { type: "asset.bind", asset: { ...old, id: "other-red" } },
        },
        {
          type: "base.apply",
          operation: {
            type: "clip.insert",
            sequence_id: other,
            track_id: otherSequence.tracks[0].id,
            clip: {
              ...red.project.base.sequences
                .find((s) => s.id === main)
                .tracks.flatMap((t) => t.clips)[0],
              id: "other-red-clip",
              asset_id: "other-red",
            },
          },
        },
      ],
    });
    await page.reload();
    await page
      .getByLabel("Active sequence", { exact: true })
      .selectOption(other);
    await back();
    await pixels(name, "red", "project-cover-active.png");
    expect(
      frames
        .filter(
          (f) => f.project_id === projectId && f.revision === otherRed.revision,
        )
        .every((f) => f.sequence_id === other),
    ).toBe(true);
    check(
      "F05: cover follows Classic's last active scene at its first frame, persists through reload, and keeps the native main-scene identity unchanged",
    );
    await page.reload();
    await pixels(name, "red", "project-cover-active-reloaded.png");
    await card(name).locator("summary").click();
    await card(name)
      .getByRole("button", { name: "Duplicate project", exact: true })
      .click();
    const copyName = `(1) ${name}`;
    results.copy = await pixels(copyName, "red", "project-cover-copy.png");
    const projects = await tool("opencut_project_list", {
      cursor: null,
      limit: 100,
    });
    const copy = projects.items.find((p) => p.name === copyName);
    expect(copy.project_id).not.toBe(projectId);
    expect(
      frames.some(
        (f) => f.project_id === copy.project_id && f.revision === copy.revision,
      ),
    ).toBe(true);
    await card(name).locator("summary").click();
    await card(name)
      .getByRole("button", { name: "Delete project", exact: true })
      .click();
    const deletion = page.getByRole("dialog", {
      name: "Delete projects",
      exact: true,
    });
    await deletion.getByLabel('Type "DELETE" to confirm').fill("DELETE");
    await deletion
      .getByRole("button", { name: "Delete projects", exact: true })
      .click();
    await expect(card(name)).toHaveCount(0);
    await page.reload();
    await pixels(copyName, "red", "project-cover-copy-after-delete.png");
    expect(
      (await inspect(copy.project_id)).project.base.assets.length,
    ).toBeGreaterThan(0);
    check(
      "F05: copied project inherits the captured cover scene with its own native frame identity; red cover and managed media survive original deletion and reload",
    );
    const route = "**/api/tools/opencut_job_frame";
    await page.route(route, async (r) => {
      if (r.request().postDataJSON().project_id !== copy.project_id)
        return r.continue();
      await r.fulfill({
        status: 503,
        contentType: "application/json",
        body: JSON.stringify({
          request_id: "f05-controlled-failure",
          data: null,
          error: {
            code: "BACKEND_UNAVAILABLE",
            message: "Controlled cover failure",
            details: null,
          },
          warnings: [],
        }),
      });
    });
    await refresh(copyName);
    await expect(
      card(copyName).getByText("Cover unavailable", { exact: true }),
    ).toBeVisible();
    await expect(card(copyName).locator(".project-cover img")).toHaveCount(0);
    const beforeRetry = await inspect(copy.project_id);
    await page.unroute(route);
    await refresh(copyName);
    await pixels(copyName, "red", "project-cover-retried.png");
    expect((await inspect(copy.project_id)).revision).toBe(
      beforeRetry.revision,
    );
    check(
      "F05: controlled frame-acceptance failure falls back without a content edit; discoverable refresh action succeeds on retry",
    );
    expect(errors).toEqual([]);
    await page.screenshot({
      path: path.join(directory, "project-covers.png"),
      fullPage: true,
    });
  } catch (e) {
    await page.screenshot({
      path: path.join(directory, "project-covers-failure.png"),
      fullPage: true,
    });
    throw e;
  } finally {
    await fs.writeFile(
      path.join(directory, "project-covers-trace.json"),
      JSON.stringify({ projectId, calls, frames, errors, results }, null, 2),
    );
    await page.close();
  }
}
