import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { createHash, randomUUID } from "node:crypto";

const PACKAGE = "/private/tmp/opencut-d1-package-20261014";
const SOURCE =
  "/private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-web-bsfcr_6o";
const CHECKPOINT = "d1-package-checkpoint.json";
const sha = (bytes) => createHash("sha256").update(bytes).digest("hex");
const read = async (filename) =>
  JSON.parse(await fs.readFile(filename, "utf8"));
const prefix = (label) =>
  new RegExp(`^${label.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}(?:\\s|$)`);
const scope = {
  self_contained_resources: true,
  history_included: false,
  fully_reproducible: false,
};
const generatedFonts = (project) =>
  project.fonts.filter((font) => font.origin.kind === "generated");
const sorted = (values) =>
  values.sort((a, b) =>
    a.resource_id < b.resource_id ? -1 : a.resource_id > b.resource_id ? 1 : 0,
  );
const inventory = (project, kind) =>
  sorted(
    (kind === "asset" ? project.assets : project.fonts).map((resource) => {
      const version = kind === "asset" ? resource.version : resource.content;
      return {
        resource_id: resource.id,
        content_hash: version.content_hash,
        size_bytes: Number(version.size),
        path: `${kind === "asset" ? "media" : "fonts"}/${version.content_hash}`,
      };
    }),
  );

// Public inspect deliberately hides private provenance. This fixture only needs
// the LegacyLocator -> Legacy rule from project_v4/public/projection.rs. Preserve
// and compare every other field; unfamiliar local/online sanitization must fail
// for a separate source inspection rather than silently erasing differences.
function publicPackageProject(raw, expected) {
  const project = structuredClone(raw),
    projectedOrigins = [];
  for (const collection of ["assets", "fonts"]) {
    for (const resource of project[collection]) {
      const publicResource = expected[collection].find(
        (value) => value.id === resource.id,
      );
      expect(
        publicResource,
        `Known ${collection} resource ${resource.id}`,
      ).toBeTruthy();
      if (resource.origin.kind === "legacy_locator") {
        expect(collection).toBe("assets");
        expect(Object.keys(resource.origin).sort()).toEqual(["kind", "uri"]);
        expect(
          resource.origin.uri,
          "Only the retained synthetic media locator is projected",
        ).toMatch(/^opencut-media:\/\/media_[a-f0-9]{32}$/);
        expect(publicResource.origin).toEqual({ kind: "legacy" });
        projectedOrigins.push({
          resource_id: resource.id,
          stored_kind: "legacy_locator",
          public_kind: "legacy",
          private_locator_retained_in_package: true,
        });
        resource.origin = { kind: "legacy" };
      } else {
        // Current fixture fonts use public pinned Google URLs or safe generated
        // IDs. No basename, signed URL or generated-ID differences are waived.
        expect(
          resource.origin,
          `Unchanged provenance for ${resource.id}`,
        ).toEqual(publicResource.origin);
      }
    }
  }
  return { project, projectedOrigins };
}

async function candidateIdentity() {
  expect(process.env.OPENCUT_WEB_BINARY).toBeTruthy();
  expect(process.env.OPENCUT_WEB_ROOT).toBeTruthy();
  const entries = [];
  async function visit(relative = "") {
    for (const entry of await fs.readdir(
      path.join(process.env.OPENCUT_WEB_ROOT, relative),
      { withFileTypes: true },
    )) {
      const name = path.join(relative, entry.name);
      if (entry.isSymbolicLink())
        throw Error(`Unhashed candidate symlink ${name}`);
      if (entry.isDirectory()) await visit(name);
      else if (/\.(js|mjs|cjs|css)$/u.test(name))
        entries.push([
          name,
          sha(await fs.readFile(path.join(process.env.OPENCUT_WEB_ROOT, name))),
        ]);
    }
  }
  await visit();
  entries.sort((a, b) => a[0].localeCompare(b[0]));
  expect(entries.length).toBeGreaterThan(0);
  return {
    binary_sha256: sha(await fs.readFile(process.env.OPENCUT_WEB_BINARY)),
    web_js_css_tree_sha256: sha(JSON.stringify(entries)),
  };
}

/** Two serial runs against different services. Export uses the retained D0 fixed
 * revision, then restore starts with an empty project library. No fixture copy,
 * renderer submission, font download, source edit or private database access. */
export async function verifyV4Package(browser, endpoint, directory, check) {
  const mode = process.env.OPENCUT_D1_PACKAGE_PHASE;
  expect(
    ["export", "restore"],
    "Set OPENCUT_D1_PACKAGE_PHASE=export|restore",
  ).toContain(mode);
  const sourceDirectory = process.env.OPENCUT_D1_PACKAGE_SOURCE_DIR ?? SOURCE;
  const checkpointPath = path.resolve(directory, CHECKPOINT);
  const candidate = await candidateIdentity();
  let facts;
  try {
    facts = await read(checkpointPath);
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
  }
  if (facts) {
    expect(facts.mode).toBe(mode);
    expect(
      facts.candidate,
      "Retain failed evidence; a different candidate needs an explicitly separate checkpoint/workspace",
    ).toEqual(candidate);
  } else
    facts = {
      mode,
      candidate,
      source_directory: sourceDirectory,
      package_path: PACKAGE,
      stages: {},
      phase: "bootstrap",
    };
  const page = await browser.newPage({
    locale: "en-US",
    viewport: { width: 1440, height: 1100 },
  });
  page.setDefaultTimeout(30000);
  const errors = [];
  page.on("pageerror", (error) => {
    if (errors.length < 32) errors.push(error.message.slice(0, 2048));
  });
  let phase = "open",
    queue = Promise.resolve();
  const save = (values = {}) => {
    Object.assign(facts, values, { active_phase: phase });
    const encoded = JSON.stringify(facts, null, 2) + "\n";
    if (Buffer.byteLength(encoded) > 8 * 1024 * 1024)
      throw Error("D1 checkpoint exceeds 8 MiB");
    queue = queue.then(async () => {
      await fs.writeFile(`${checkpointPath}.tmp`, encoded);
      await fs.rename(`${checkpointPath}.tmp`, checkpointPath);
    });
    return queue;
  };
  const tool = async (name, input) => {
    const envelope = await page.evaluate(
      async ({ name, input }) =>
        (
          await fetch(`/v1/tools/${name}`, {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(input),
          })
        ).json(),
      { name, input },
    );
    expect(
      envelope.error,
      `${name}: ${JSON.stringify(envelope.error)}`,
    ).toBeNull();
    return envelope.data;
  };
  const inspect = (projectId, revision = null) =>
    tool("opencut_project_v4_inspect", { project_id: projectId, revision });
  const observe = async (operation, action) => {
    const waiting = page.waitForResponse((response) =>
      response.url().endsWith(`/v1/tools/${operation}`),
    );
    await action();
    const response = await waiting,
      envelope = await response.json();
    expect(envelope.error, JSON.stringify(envelope.error)).toBeNull();
    return { request: response.request().postDataJSON(), data: envelope.data };
  };
  // Persist the original UI request before releasing it to the service.
  const ui = async (stage, operation, action) => {
    const pattern = `**/v1/tools/${operation}`;
    const capture = async (route) => {
      facts.stages[stage] = {
        operation,
        request: route.request().postDataJSON(),
        via: "ui",
      };
      await save({ pending: stage });
      await route.continue();
    };
    await page.route(pattern, capture, { times: 1 });
    try {
      const result = await observe(operation, action);
      facts.stages[stage].receipt = result.data;
      await save({ pending: null });
      return result;
    } finally {
      await page.unroute(pattern, capture);
    }
  };
  const replay = async (stage) => {
    const retained = facts.stages[stage];
    expect(retained?.request).toBeTruthy();
    const result = await tool(retained.operation, retained.request);
    if (retained.receipt) expect(result).toEqual(retained.receipt);
    retained.receipt = result;
    await save({ pending: null });
    return result;
  };
  const closeWelcome = async () => {
    const welcome = page.getByRole("dialog", {
      name: "Welcome to OpenCut",
      exact: true,
    });
    if (await welcome.isVisible())
      await welcome.getByRole("button", { name: "Close", exact: true }).click();
  };
  const openProject = async (id, sequenceId) => {
    await page.goto(
      `${endpoint}/editor/${encodeURIComponent(id)}${sequenceId ? `?scene=${encodeURIComponent(sequenceId)}` : ""}`,
    );
    await expect(
      page.getByRole("main", { name: "V4 editor", exact: true }),
    ).toBeVisible();
    await closeWelcome();
    if (sequenceId)
      await page
        .getByRole("combobox", { name: prefix("V4 scene") })
        .selectOption(sequenceId);
  };
  const projectIds = async () => {
    const ids = new Set();
    for (const name of ["opencut_project_library", "opencut_project_v4_list"]) {
      let cursor = null;
      for (let i = 0; i < 100; i++) {
        const result = await tool(name, { cursor, limit: 100 });
        result.items.forEach((item) => ids.add(item.project_id));
        cursor = result.next_cursor ?? null;
        if (!cursor) break;
        if (i === 99) throw Error("Project inventory exceeds verifier bound");
      }
    }
    return [...ids].sort();
  };
  const proofs = async (snapshot) => {
    const values = [];
    for (const font of generatedFonts(snapshot.project)) {
      const value = await tool("opencut_font_derived_v1_proof", {
        project_id: snapshot.project.id,
        revision: snapshot.revision,
        font_id: font.id,
      });
      expect(value).toMatchObject({
        schema_version: 1,
        project_id: snapshot.project.id,
        revision: snapshot.revision,
        font_id: font.id,
      });
      expect(value.source.face.content_hash).toBe(font.content.content_hash);
      expect(Number(value.source.face.size_bytes)).toBe(
        Number(font.content.size),
      );
      values.push(value);
    }
    return values;
  };
  const validatePackage = async (packagePath, source, receipt) => {
    const inspection = await tool("opencut_project_v4_package_inspect", {
      package_path: packagePath,
    });
    expect(inspection).toMatchObject({
      ...scope,
      source_project_id: source.project_id,
      source_revision: source.revision,
      asset_count: source.project.assets.length,
      font_count: source.project.fonts.length,
    });
    const manifestBytes = await fs.readFile(
      path.join(packagePath, "manifest.json"),
    );
    expect(sha(manifestBytes)).toBe(inspection.manifest_sha256);
    if (receipt)
      expect(receipt).toMatchObject({
        ...scope,
        manifest_sha256: inspection.manifest_sha256,
        source_revision: source.revision,
        asset_count: inspection.asset_count,
        font_count: inspection.font_count,
        derived_font_count: inspection.derived_font_count,
      });
    const manifest = JSON.parse(manifestBytes);
    expect(manifest).toMatchObject({
      ...scope,
      schema_version: 6,
      project_schema_version: 4,
      source_project_id: source.project_id,
      source_revision: source.revision,
      project_path: "project.json",
    });
    const projectBytes = await fs.readFile(
      path.join(packagePath, "project.json"),
    );
    const rawProjectSha256 = sha(projectBytes);
    expect(rawProjectSha256).toBe(manifest.project_sha256);
    const comparison = publicPackageProject(
      JSON.parse(projectBytes),
      source.project,
    );
    expect(comparison.project).toEqual(source.project);
    facts.package_documents ??= {};
    facts.package_documents[packagePath] = {
      raw_project_sha256: rawProjectSha256,
      raw_project_size_bytes: projectBytes.length,
      source_revision: source.revision,
      origin_projection: comparison.projectedOrigins,
      comparison:
        "Complete document equals public fixed inspect after the existing LegacyLocator-to-Legacy projection; private media locators remain in the hashed package and are not exposed by public inspect. All other fields, including font origins and licenses, compare exactly.",
    };
    await save();
    expect(manifest.assets).toEqual(inventory(source.project, "asset"));
    expect(manifest.fonts).toEqual(inventory(source.project, "font"));
    const expectedProofs = [
      ...new Map(
        source.proofs.map((proof) => [
          proof.source.face.derivation_id,
          proof.source,
        ]),
      ).values(),
    ].sort((a, b) => (a.face.derivation_id < b.face.derivation_id ? -1 : 1));
    expect(manifest.derived_fonts).toEqual(expectedProofs);
    expect(inspection.derived_font_count).toBe(expectedProofs.length);
    return inspection;
  };
  const validateRestored = async (source, receipt) => {
    const snapshot = await inspect(receipt.target_project_id, receipt.revision);
    const expected = structuredClone(source.project);
    expected.id = receipt.target_project_id;
    expect(snapshot.project).toEqual(expected);
    expect(receipt).toMatchObject({
      ...scope,
      source_project_id: source.project_id,
      source_revision: source.revision,
      asset_count: expected.assets.length,
      font_count: expected.fonts.length,
    });
    const observed = [];
    for (const kind of ["asset", "font"])
      for (const resource of inventory(expected, kind)) {
        const descriptor = await tool("opencut_project_v4_resource_inspect", {
          project_id: expected.id,
          revision: snapshot.revision,
          kind,
          resource_id: resource.resource_id,
        });
        expect(descriptor).toMatchObject({
          project_id: expected.id,
          revision: snapshot.revision,
          kind,
          resource_id: resource.resource_id,
          version: {
            content_hash: resource.content_hash,
            size: resource.size_bytes,
          },
        });
        const url = new URL(descriptor.content_path, endpoint);
        expect(url.origin).toBe(new URL(endpoint).origin);
        expect(url.pathname.startsWith("/v1/")).toBe(true);
        expect(
          resource.size_bytes,
          "Owned acceptance resources must fit the bounded verifier read",
        ).toBeLessThanOrEqual(128 * 1024 * 1024);
        const response = await page.request.get(url.href);
        expect(
          response.ok(),
          `Resource ${kind}/${resource.resource_id}: HTTP ${response.status()}`,
        ).toBe(true);
        const bytes = await response.body();
        expect(bytes.length).toBe(resource.size_bytes);
        expect(sha(bytes)).toBe(resource.content_hash);
        observed.push({
          kind,
          resource_id: resource.resource_id,
          sha256: sha(bytes),
          size_bytes: bytes.length,
        });
        await response.dispose();
      }
    const restoredProofs = await proofs(snapshot);
    expect(
      restoredProofs.map((value) => ({
        font_id: value.font_id,
        source: value.source,
      })),
    ).toEqual(
      source.proofs.map((value) => ({
        font_id: value.font_id,
        source: value.source,
      })),
    );
    expect(
      new Set(restoredProofs.map((value) => value.source.face.derivation_id))
        .size,
    ).toBe(receipt.derived_font_count);
    return {
      resources: observed,
      proofs: restoredProofs,
      graph_equal_except_project_id: true,
    };
  };

  try {
    await page.goto(`${endpoint}/projects`);
    await expect(
      page.getByRole("button", {
        name: "Restore project package",
        exact: true,
      }),
    ).toBeEnabled();
    await closeWelcome();
    if (mode === "export") {
      phase = "select retained fixed source";
      const d0 = await read(
        path.join(sourceDirectory, "b4-graphic-output-checkpoint.json"),
      );
      expect(d0.phase).toBe("passed");
      expect(d0.revision).toBeTruthy();
      const snapshot = await inspect(d0.project_id, d0.revision);
      expect(snapshot.revision).toBe(d0.revision);
      if (!facts.source) {
        await save({
          source: {
            project_id: snapshot.project.id,
            revision: snapshot.revision,
            project: snapshot.project,
            proofs: await proofs(snapshot),
          },
          sequence_id: d0.sequence_id,
          source_head: await inspect(d0.project_id),
        });
      }
      expect(snapshot.project).toEqual(facts.source.project);
      expect((await inspect(d0.project_id)).revision).toBe(
        facts.source_head.revision,
      );
      if (!facts.exported) {
        // UI does not print revision IDs. Match the exact public history page to
        // its rendered ordered rows, then verify the clicked inspect response.
        const first = await observe("opencut_project_history", () =>
          openProject(d0.project_id, d0.sequence_id),
        );
        const revisions = [...first.data.revisions];
        let cursor = first.data.next_cursor;
        while (
          !revisions.some((revision) => revision.revision === d0.revision) &&
          cursor
        ) {
          expect(revisions.length, "Bounded history traversal").toBeLessThan(
            2000,
          );
          const older = await observe("opencut_project_history", () =>
            page
              .locator(".v4-history")
              .getByRole("button", { name: "Older revisions", exact: true })
              .click(),
          );
          expect(older.request.from_revision).toBe(cursor);
          for (const row of older.data.revisions)
            if (!revisions.some((item) => item.revision === row.revision))
              revisions.push(row);
          cursor = older.data.next_cursor;
        }
        const index = revisions.findIndex(
          (revision) => revision.revision === d0.revision,
        );
        expect(
          index,
          "Fixed D0 revision must be selectable from public UI history",
        ).toBeGreaterThanOrEqual(0);
        const rows = page.locator(".v4-history .v4-revisions > li");
        await expect(rows).toHaveCount(revisions.length);
        const historical = await ui(
          "view-source-revision",
          "opencut_project_v4_inspect",
          () =>
            rows
              .nth(index)
              .getByRole("button", { name: "View revision", exact: true })
              .click(),
        );
        expect(historical.request).toEqual({
          project_id: d0.project_id,
          revision: d0.revision,
        });
        expect(historical.data).toEqual(snapshot);
        await expect(
          page.getByText("Viewing a saved revision · Read only", {
            exact: false,
          }),
        ).toBeVisible();
        await page
          .getByRole("button", { name: "Export project package", exact: true })
          .click();
        const dialog = page.getByRole("dialog", {
          name: "Export project package",
          exact: true,
        });
        await dialog
          .getByRole("textbox", { name: prefix("New package folder") })
          .fill(PACKAGE);
        await fs.mkdir(path.dirname(PACKAGE), { recursive: true });
        if (facts.stages.export?.request) {
          // Export publishes a new directory and is not an idempotent mutation.
          // Resolve an uncertain UI attempt by inspecting the existing result.
          const inspection = await validatePackage(PACKAGE, facts.source);
          await save({
            exported: inspection,
            export_recovered_by_inspection: true,
          });
          await dialog
            .getByRole("button", { name: "Close", exact: true })
            .last()
            .click();
        } else {
          const result = await ui(
            "export",
            "opencut_project_v4_package_export",
            () =>
              dialog
                .getByRole("button", { name: "Save package", exact: true })
                .click(),
          );
          expect(result.request).toEqual({
            project_id: d0.project_id,
            revision: d0.revision,
            destination: PACKAGE,
          });
          await expect(
            dialog.getByText("Project package saved and verified", {
              exact: true,
            }),
          ).toBeVisible();
          await save({ exported: result.data, export_ui_passed: true });
          await dialog
            .getByRole("button", { name: "Done", exact: true })
            .click();
        }
      }
      phase = "verify exported typed inventory";
      await save({
        inspection: await validatePackage(
          PACKAGE,
          facts.source,
          facts.exported,
        ),
      });
      if (!facts.source.proofs.length) {
        phase = "export retained derived-font fixed revision";
        const old = await read(
          path.join(sourceDirectory, "f20-derived-font-checkpoint.json"),
        );
        const derived = await inspect(old.project_id, old.render_revision);
        const source = {
          project_id: old.project_id,
          revision: derived.revision,
          project: derived.project,
          proofs: await proofs(derived),
        };
        expect(
          source.proofs.find((proof) => proof.font_id === old.font_id)?.source,
        ).toEqual(old.proof.source);
        expect(source.proofs.length).toBeGreaterThan(0);
        if (!facts.derived)
          await save({
            derived: { source, package_path: `${PACKAGE}-derived` },
          });
        expect(facts.derived.source).toEqual(source);
        if (!facts.derived.exported) {
          if (!facts.stages["export-derived"]) {
            facts.stages["export-derived"] = {
              operation: "opencut_project_v4_package_export",
              request: {
                project_id: source.project_id,
                revision: source.revision,
                destination: facts.derived.package_path,
              },
              via: "public_tool",
            };
            await save({ pending: "export-derived" });
            facts.derived.exported = await tool(
              "opencut_project_v4_package_export",
              facts.stages["export-derived"].request,
            );
          } else
            facts.derived.exported = await validatePackage(
              facts.derived.package_path,
              source,
            );
          await save({ pending: null });
        }
        facts.derived.inspection = await validatePackage(
          facts.derived.package_path,
          source,
          facts.derived.exported,
        );
        await save();
      }
      expect(await inspect(facts.source.project_id)).toEqual(facts.source_head);
      await save({
        source_head_unchanged: true,
        phase: "passed",
        pageerrors: errors,
      });
      // Small pointer outside the immutable package, usable by a distinct empty
      // workspace even when the export evidence directory differs from SOURCE.
      await fs.writeFile(
        `${PACKAGE}.verification.json`,
        JSON.stringify({
          checkpoint_path: checkpointPath,
          manifest_sha256: facts.inspection.manifest_sha256,
        }) + "\n",
      );
    } else {
      phase = "verify empty restore service";
      const pointer = await read(`${PACKAGE}.verification.json`),
        exported = await read(pointer.checkpoint_path);
      expect(exported.phase).toBe("passed");
      expect(exported.mode).toBe("export");
      expect(exported.candidate).toEqual(candidate);
      expect(pointer.manifest_sha256).toBe(exported.inspection.manifest_sha256);
      expect(path.resolve(directory)).not.toBe(
        path.dirname(pointer.checkpoint_path),
      );
      if (!facts.empty_workspace) {
        expect(
          await projectIds(),
          "Restore must start in a genuinely empty service, not the source workspace",
        ).toEqual([]);
        await save({
          empty_workspace: {
            project_ids: [],
            checked_before_package_inspect: true,
          },
          source: exported.source,
          sequence_id: exported.sequence_id,
          exported_manifest_sha256: pointer.manifest_sha256,
        });
      }
      expect(facts.source).toEqual(exported.source);
      const expectedIds = [
        facts.stages.import?.request.target_project_id,
        facts.stages["import-derived"]?.request.target_project_id,
      ]
        .filter(Boolean)
        .sort();
      const before = await projectIds();
      expect(
        before.every((id) => expectedIds.includes(id)),
        "Only original checkpointed restores may exist when resuming",
      ).toBe(true);
      if (!facts.restored) {
        if (facts.stages.import?.request) {
          await save({
            import_request: facts.stages.import.request,
            restored: await replay("import"),
          });
          await openProject(
            facts.restored.target_project_id,
            facts.sequence_id,
          );
          await save({
            restore_open_resume:
              "original request replay, existing project route",
          });
        } else {
          await page
            .getByRole("button", {
              name: "Restore project package",
              exact: true,
            })
            .click();
          const dialog = page.getByRole("dialog", {
            name: "Restore project package",
            exact: true,
          });
          await dialog
            .getByRole("textbox", { name: prefix("Package folder") })
            .fill(PACKAGE);
          const reviewed = await ui(
            "inspect",
            "opencut_project_v4_package_inspect",
            () =>
              dialog
                .getByRole("button", { name: "Check package", exact: true })
                .click(),
          );
          expect(reviewed.data).toEqual(exported.inspection);
          expect(await projectIds()).toEqual([]);
          await expect(
            dialog.getByRole("heading", {
              name: facts.source.project.name,
              exact: true,
            }),
          ).toBeVisible();
          const imported = await ui(
            "import",
            "opencut_project_v4_package_import",
            () =>
              dialog
                .getByRole("button", {
                  name: "Restore as new project",
                  exact: true,
                })
                .click(),
          );
          expect(imported.request).toMatchObject({
            package_path: PACKAGE,
            expected_manifest_sha256: pointer.manifest_sha256,
          });
          expect(imported.data.target_project_id).toBe(
            imported.request.target_project_id,
          );
          await save({
            import_request: imported.request,
            restored: imported.data,
          });
          await expect(
            dialog.getByText("Project restored", { exact: true }),
          ).toBeVisible();
          await dialog
            .getByRole("button", { name: "Open restored project", exact: true })
            .click();
          await expect(
            page.getByRole("main", { name: "V4 editor", exact: true }),
          ).toBeVisible();
          await save({ restore_ui_passed: true });
        }
      }
      phase = "verify actual restored resource bytes and original replay";
      expect(facts.restored.manifest_sha256).toBe(pointer.manifest_sha256);
      expect(facts.restored.target_project_id).not.toBe(
        facts.source.project_id,
      );
      if (!facts.resource_verification)
        await save({
          resource_verification: await validateRestored(
            facts.source,
            facts.restored,
          ),
        });
      expect(await replay("import")).toEqual(facts.restored);
      const afterMain = await projectIds();
      expect(
        afterMain.filter((id) => id === facts.restored.target_project_id),
      ).toHaveLength(1);
      await save({ original_import_replay_passed: true });
      if (exported.derived) {
        phase = "restore and verify retained derived-font package";
        const source = exported.derived.source;
        expect(
          await validatePackage(exported.derived.package_path, source),
        ).toEqual(exported.derived.inspection);
        if (!facts.stages["import-derived"]) {
          facts.stages["import-derived"] = {
            operation: "opencut_project_v4_package_import",
            request: {
              package_path: exported.derived.package_path,
              expected_manifest_sha256:
                exported.derived.inspection.manifest_sha256,
              target_project_id: randomUUID(),
              idempotency_key: randomUUID(),
            },
            via: "public_tool",
          };
          await save({ pending: "import-derived" });
        }
        const receipt = await replay("import-derived");
        if (!facts.derived) {
          facts.derived = {
            package_path: exported.derived.package_path,
            source_project_id: source.project_id,
            source_revision: source.revision,
            restored: receipt,
            verification: await validateRestored(source, receipt),
          };
          await save();
        }
        expect(receipt.derived_font_count).toBeGreaterThan(0);
        expect(await replay("import-derived")).toEqual(receipt);
      }
      phase = "edit restored graphic through real UI";
      const id = facts.restored.target_project_id;
      if (!facts.edit_passed) {
        await openProject(id, facts.sequence_id);
        if (!facts.edit_baseline) {
          const baseline = await inspect(id);
          const expected = structuredClone(facts.source.project);
          expected.id = id;
          expect(baseline.project).toEqual(expected);
          await save({ edit_baseline: baseline });
        }
        const sequence = facts.edit_baseline.project.sequences.find(
          (scene) => scene.id === facts.sequence_id,
        );
        const element = sequence.tracks
          .flatMap((track) => track.elements)
          .find(
            (value) =>
              value.content.kind === "graphic" &&
              value.content.graphic.geometry.kind === "rectangle",
          );
        expect(element).toBeTruthy();
        const rounding =
          element.content.graphic.geometry.corner_radius_percent === 25
            ? 30
            : 25;
        let receipt;
        if (facts.stages.edit?.request) receipt = await replay("edit");
        else {
          await page
            .locator(`[data-element-id="${element.id}"] .v4-element-select`)
            .click({ position: { x: 10, y: 10 } });
          await page
            .getByRole("button", { name: "Edit graphic", exact: true })
            .click();
          const form = page.getByRole("form", {
            name: "Edit graphic",
            exact: true,
          });
          await form
            .getByRole("textbox", { name: prefix("Corner radius (percent)") })
            .fill(String(rounding));
          const plan = await ui("edit-plan", "opencut_graphic_v1_plan", () =>
            form
              .getByRole("button", {
                name: "Review graphic changes",
                exact: true,
              })
              .click(),
          );
          expect(await inspect(id)).toEqual(facts.edit_baseline);
          const edit = await ui("edit", "opencut_edit_v4_apply", () =>
            page
              .getByRole("button", {
                name: "Apply graphic changes",
                exact: true,
              })
              .click(),
          );
          expect(edit.request).toEqual(plan.data.transaction);
          receipt = edit.data;
        }
        const expected = structuredClone(facts.edit_baseline.project);
        expected.sequences
          .find((scene) => scene.id === sequence.id)
          .tracks.flatMap((track) => track.elements)
          .find(
            (value) => value.id === element.id,
          ).content.graphic.geometry.corner_radius_percent = rounding;
        expect((await inspect(id, receipt.revision)).project).toEqual(expected);
        await page.reload();
        await expect(
          page.getByRole("main", { name: "V4 editor", exact: true }),
        ).toBeVisible();
        expect((await inspect(id)).project).toEqual(expected);
        await save({
          edit_passed: true,
          edited_revision: receipt.revision,
          edited_graphic_id: element.id,
        });
      }
      phase = "undo restored edit and reload complete graph";
      if (!facts.undo_passed) {
        await openProject(id, facts.sequence_id);
        let receipt;
        if (facts.stages.undo?.request) receipt = await replay("undo");
        else {
          const history = page.locator(".v4-history");
          const plan = await ui("undo-plan", "opencut_edit_v4_undo_plan", () =>
            history.getByRole("button", { name: "Undo", exact: true }).click(),
          );
          const undone = await ui("undo", "opencut_edit_v4_undo_apply", () =>
            history
              .getByRole("button", { name: "Apply undo", exact: true })
              .click(),
          );
          expect(undone.request).toEqual(plan.request);
          receipt = undone.data;
        }
        expect(
          (await inspect(id, receipt.transaction.revision)).project,
        ).toEqual(facts.edit_baseline.project);
        await page.reload();
        await expect(
          page.getByRole("main", { name: "V4 editor", exact: true }),
        ).toBeVisible();
        expect((await inspect(id)).project).toEqual(
          facts.edit_baseline.project,
        );
        await save({ undo_passed: true });
      }
      for (const [stage, direction] of [
        ["redo", "redo"],
        ["final-undo", "undo"],
      ]) {
        if (facts[`${stage}_passed`]) continue;
        phase = `${direction} restored edit and reload`;
        await openProject(id, facts.sequence_id);
        let receipt;
        if (facts.stages[stage]?.request) receipt = await replay(stage);
        else {
          const history = page.locator(".v4-history");
          const plan = await ui(
            `${stage}-plan`,
            `opencut_edit_v4_${direction}_plan`,
            () =>
              history
                .getByRole("button", {
                  name: direction === "undo" ? "Undo" : "Redo",
                  exact: true,
                })
                .click(),
          );
          const applied = await ui(
            stage,
            `opencut_edit_v4_${direction}_apply`,
            () =>
              history
                .getByRole("button", {
                  name: `Apply ${direction}`,
                  exact: true,
                })
                .click(),
          );
          expect(applied.request).toEqual(plan.request);
          receipt = applied.data;
        }
        const expected =
          direction === "redo"
            ? (await inspect(id, facts.edited_revision)).project
            : facts.edit_baseline.project;
        expect(
          (await inspect(id, receipt.transaction.revision)).project,
        ).toEqual(expected);
        await page.reload();
        await expect(
          page.getByRole("main", { name: "V4 editor", exact: true }),
        ).toBeVisible();
        expect((await inspect(id)).project).toEqual(expected);
        await save({ [`${stage}_passed`]: true });
      }
      const final = await inspect(id),
        expected = structuredClone(facts.source.project);
      expected.id = id;
      expect(final.project).toEqual(expected);
      expect(await projectIds()).toEqual(
        [
          facts.restored.target_project_id,
          facts.derived?.restored.target_project_id,
        ]
          .filter(Boolean)
          .sort(),
      );
      await save({
        restored_render_revision: final.revision,
        phase: "passed",
        pageerrors: errors,
      });
    }
    expect(errors).toEqual([]);
    await page.screenshot({
      path: path.join(directory, `d1-package-${mode}.png`),
      fullPage: true,
    });
    check(
      `D1 V4 package ${mode}: fixed graph, resources, font proofs and original requests`,
      facts,
    );
    return facts;
  } catch (error) {
    await save({
      phase: "failed",
      failure: { phase, error: String(error).slice(0, 4096) },
      pageerrors: errors,
    });
    await page
      .screenshot({
        path: path.join(directory, `d1-package-${mode}-failure.png`),
        fullPage: true,
      })
      .catch(() => {});
    throw error;
  } finally {
    await queue;
    await page.close();
  }
}
