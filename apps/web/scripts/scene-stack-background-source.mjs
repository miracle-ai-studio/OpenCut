// Explicit supplemental synthetic source for a demonstrated flat-source gap.
// Bytes travel through the public upload endpoint; every write is resumable.
import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { createHash, randomUUID } from "node:crypto";

export async function bindBackgroundSource({ page, endpoint, file, facts, operation, inspect, pollJob, tool, save }) {
  const bytes = await fs.readFile(file);
  expect(bytes.length).toBeGreaterThan(0);
  expect(bytes.length).toBeLessThanOrEqual(1024 * 1024);
  const hash = createHash("sha256").update(bytes).digest("hex");
  const source = facts.background_source ??= {
    path: path.resolve(file), sha256: hash, asset_id: randomUUID(),
    reason: "Original retained source is spatially flat; supplemental source is independently authored, not product output",
  };
  expect(source.path).toBe(path.resolve(file));
  expect(source.sha256).toBe(hash);
  await save();
  const upload = await operation("background-upload", "opencut_upload_begin", {
    project_id: facts.project_id, idempotency_key: `background-upload-${facts.project_id}`,
    name: path.basename(file), size_bytes: String(bytes.length), content_hash: hash,
  });
  if (!source.chunk_confirmed) {
    const query = new URLSearchParams({ size_bytes: String(bytes.length), content_hash: hash });
    const response = await page.request.put(`${endpoint}/api/uploads/${upload.upload_id}/chunks/0?${query}`, {
      headers: { "Content-Type": "application/octet-stream", Origin: new URL(endpoint).origin }, data: bytes,
    });
    const result = await response.json();
    expect(result.error, JSON.stringify(result.error)).toBeNull();
    source.chunk_confirmed = true;
    await save();
  }
  const sealed = await operation("background-seal", "opencut_upload_complete", { upload_id: upload.upload_id, content_hash: upload.content_hash });
  const job = await pollJob(tool, sealed.job_id);
  expect(job.result.kind).toBe("media.import");
  const imported = job.result.data;
  expect(imported.asset.version.content_hash).toBe(hash);
  source.import_job_id = job.job_id;
  if (!facts.stages["background-bind-plan"]) {
    const head = await inspect();
    facts.stages["background-bind-plan"] = { name: "opencut_media_import_plan", input: {
      project_id: facts.project_id, expected_revision: head.revision, sequence_id: facts.sequence_id,
      idempotency_key: `background-bind-${facts.project_id}`, label: "Independent spatial background source",
      bindings: [{ media_id: imported.media_id, version: imported.asset.version,
        asset_id: source.asset_id, name: "Independent grayscale checkerboard", mode: "bind" }],
      placements: [], ripple: false,
    } };
    await save();
  }
  const plan = await operation("background-bind-plan", "opencut_media_import_plan", facts.stages["background-bind-plan"].input);
  const applied = await operation("background-bind", "opencut_edit_apply", plan.transaction);
  const fixed = await inspect(facts.project_id, applied.revision);
  const asset = fixed.project.assets.find(a => a.id === source.asset_id);
  expect(asset.version.content_hash).toBe(hash);
  await save();
  return asset;
}
