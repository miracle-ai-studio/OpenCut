import { editorSnapshot } from "./project-inspection.mjs";
// Real Classic recovery UI -> direct V4, preserving every basic mask field.
import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { createHash } from "node:crypto";
const sha = (bytes) => createHash("sha256").update(bytes).digest("hex");
export async function verifyClassicMaskImport(page, endpoint, directory, tool) {
  const checkpoint = path.join(directory, "classic-mask-v4-checkpoint.json");
  let facts;
  try {
    facts = JSON.parse(await fs.readFile(checkpoint, "utf8"));
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
  }
  const save = async () =>
    fs.writeFile(checkpoint, JSON.stringify(facts, null, 2) + "\n");
  if (!facts) {
    const raw = JSON.parse(
      await fs.readFile(
        new URL(
          "../../../crates/editor-application/src/classic_import_v4/fixture.json",
          import.meta.url,
        ),
        "utf8",
      ),
    );
    raw.metadata = {
      id: "classic-mask-v4-static-v1",
      name: "Classic rectangle ellipse masks",
    };
    raw.scenes[0].tracks.overlay = ["rectangle", "ellipse"].map((kind, i) => ({
      id: `mask-track-${kind}`,
      name: kind,
      type: "graphic",
      hidden: false,
      elements: [
        {
          id: `mask-shape-${kind}`,
          name: kind,
          type: "graphic",
          definitionId: "rectangle",
          startTime: 0,
          duration: 360000,
          trimStart: 12000,
          trimEnd: 24000,
          sourceDuration: 396000,
          params: {
            fill: "#cccccc",
            stroke: "#ffffff",
            strokeWidth: 0,
            strokeAlign: "inside",
            cornerRadius: 0,
            "transform.positionX": i ? 70 : -70,
            "transform.positionY": 0,
            "transform.scaleX": i ? -0.22 : 0.22,
            "transform.scaleY": 0.15,
            "transform.rotate": i ? 17 : 0,
            opacity: 0.75,
            blendMode: "normal",
          },
          effects: [],
          animations: {},
          masks: [
            {
              id: `classic-${kind}`,
              type: kind,
              params: {
                centerX: 0.125,
                centerY: -0.075,
                width: 0.625,
                height: 0.375,
                rotation: -23.5,
                scale: 2.125,
                feather: i ? 24 : 0,
                inverted: false,
                strokeColor: "#33669980",
                strokeWidth: 0,
                strokeAlign: "outside",
              },
            },
          ],
        },
      ],
    }));
    const snapshot = {
      origin: "http://classic.invalid",
      sourceId: raw.metadata.id,
      raw,
      context: { legacyTracksBySceneId: {}, mediaTypesById: {} },
      media: [],
    };
    const header = Buffer.from(
        JSON.stringify({
          format: "opencut.classic-recovery",
          snapshot,
          media: [],
        }),
      ),
      length = Buffer.alloc(4);
    length.writeUInt32LE(header.length);
    const bytes = Buffer.concat([
      Buffer.from("OPENCUT-CLASSIC-RECOVERY\n"),
      length,
      header,
    ]);
    const bundle = path.join(directory, "classic-mask-v4.occlassic");
    await fs.writeFile(bundle, bytes);
    facts = { raw, bundle, bundle_sha256: sha(bytes), stages: {} };
    await save();
  }
  expect(sha(await fs.readFile(facts.bundle))).toBe(facts.bundle_sha256);
  const exchange = async (stage, operation, action) => {
    if (facts.stages[stage]?.envelope?.data)
      return facts.stages[stage].envelope.data;
    if (facts.stages[stage]?.request) {
      const data = await tool(operation, facts.stages[stage].request);
      facts.stages[stage].envelope = { data, error: null };
      await save();
      return data;
    }
    const matcher = `**/api/tools/${operation}`;
    const handler = async (route) => {
      facts.stages[stage] = { request: route.request().postDataJSON() };
      await save();
      await route.continue();
    };
    await page.route(matcher, handler);
    try {
      const waiting = page.waitForResponse((response) =>
        response.url().endsWith(`/api/tools/${operation}`),
      );
      await action();
      const envelope = await (await waiting).json();
      facts.stages[stage].envelope = envelope;
      await save();
      expect(envelope.error, JSON.stringify(envelope.error)).toBeNull();
      return envelope.data;
    } finally {
      await page.unroute(matcher, handler);
    }
  };
  if (!facts.imported) {
    await page.goto(`${endpoint}/editor`);
    await page
      .getByRole("button", { name: "Recover Classic projects", exact: true })
      .click();
    await page
      .getByLabel("Classic recovery bundle", { exact: true })
      .setInputFiles(facts.bundle);
    facts.analysis = await exchange(
      "analysis",
      "opencut_classic_analyze",
      () =>
        page
          .getByRole("button", { name: "Prepare recovery", exact: true })
          .click(),
    );
    expect(facts.analysis.blockers).toEqual([]);
    expect(facts.analysis.losses).toEqual([]);
    expect(facts.analysis.unresolved_fonts).toEqual([]);
    facts.imported = await exchange("import", "opencut_classic_import", () =>
      page
        .getByRole("button", { name: "Recover project", exact: true })
        .click(),
    );
    await save();
  }
  const snapshot = await editorSnapshot(tool("opencut_project_inspect", {
    project_id: facts.imported.target_project_id,
    revision: facts.imported.revision,
  }));
  expect(snapshot.project).toEqual(facts.analysis.proposed_project);
  for (const [i, kind] of ["rectangle", "ellipse"].entries()) {
    const element = snapshot.project.sequences[0].tracks.find(
      (track) => track.id === `mask-track-${kind}`,
    ).elements[0];
    expect(element.visual.masks).toEqual([
      {
        id: `classic-${kind}`,
        enabled: true,
        inverted: false,
        shape: {
          kind,
          geometry: {
            center_x: 0.125,
            center_y: -0.075,
            width: 0.625,
            height: 0.375,
            rotation_degrees: -23.5,
            scale: 2.125,
          },
        },
        feather: { unit: "classic_pixels", value: i ? 24 : 0 },
        stroke: {
          width_px: 0,
          align: "outside",
          color: {
            encoding: "srgb",
            red: 51 / 255,
            green: 102 / 255,
            blue: 153 / 255,
            alpha: 128 / 255,
          },
        },
      },
    ]);
    expect(element.timeline_range.duration).toEqual({
      ticks: "360000",
      timebase: 120000,
    });
    expect(element.source_span.trim_start).toEqual({
      ticks: "12000",
      timebase: 120000,
    });
  }
  facts.status = "passed";
  facts.revision = snapshot.revision;
  await save();
  return {
    status: facts.status,
    checkpoint,
    source_sha256: sha(JSON.stringify(facts.raw)),
    imported: facts.imported,
    mask_count: 2,
  };
}
