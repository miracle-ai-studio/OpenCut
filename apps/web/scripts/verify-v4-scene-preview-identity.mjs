import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { execFileSync } from "node:child_process";
import {
  installScenePreviewWitness,
  trackScenePreviewPumps,
} from "./verify-v4-scene-preview.mjs";
import {
  nativeTree,
  processes,
} from "./verify-v4-scene-preview-continuity.mjs";

const snapshot = (pid) => {
  const rows = nativeTree(pid, processes());
  if (rows.length > 64)
    throw Error("Identity census exceeded64 owned processes");
  return rows;
};
const toolFor = (page) => async (name, input) => {
  const reply = await page.evaluate(
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
  expect(reply.error).toBeNull();
  return reply.data;
};
async function ready(page, endpoint, id) {
  await page.goto(`${endpoint}/editor${id ? `/${id}` : ""}`);
  if (!id)
    await expect(
      page.getByRole("button", { name: "New project", exact: true }),
    ).toBeEnabled();
  else
    await expect(
      page.getByRole("main", { name: "V4 editor", exact: true }),
    ).toBeVisible();
  const guide = page.getByRole("dialog", {
    name: "Welcome to OpenCut",
    exact: true,
  });
  if (await guide.isVisible())
    await guide.getByRole("button", { name: "Close", exact: true }).click();
}
async function play(page, witness) {
  await page
    .getByRole("button", { name: "Play scene preview", exact: true })
    .click();
  await expect
    .poll(
      async () =>
        Number(
          await page
            .getByRole("img", { name: "Live scene preview", exact: true })
            .getAttribute("data-time"),
        ),
      { timeout: 15000 },
    )
    .toBeGreaterThan(0.05);
  return (await witness()).session;
}
async function exited(browser, page, owned, pid) {
  await expect(
    page.getByRole("img", { name: "Live scene preview", exact: true }),
  ).toBeHidden();
  await expect
    .poll(
      () => processes().filter((p) => owned.get(p.pid) === p.start).length,
      { timeout: 10000 },
    )
    .toBe(0);
  const states = await page.evaluate(() => window.__scenePreviewDeviceStates());
  expect(states.length).toBeGreaterThan(0);
  expect(states.every((s) => s === "closed")).toBe(true);
  const cdp = await browser.newBrowserCDPSession();
  try {
    expect(
      (await cdp.send("Target.getTargets")).targetInfos.filter(
        (t) => t.type === "worker" && /pump-worker/.test(t.url),
      ),
    ).toHaveLength(0);
  } finally {
    await cdp.detach();
  }
  return {
    captured_native_children: owned.size,
    captured_children_remaining: 0,
    after_owned_processes: snapshot(pid).length,
    device_closed: true,
    worker_targets: 0,
  };
}
// Add only the missing second scene; retain original CAS/media and fixed source
// head. This one small project is reused for future scene/history boundaries.
async function fixtureFor(tool, directory) {
  const original = JSON.parse(
    await fs.readFile(
      path.join(directory, "scene-preview-fixture.json"),
      "utf8",
    ),
  );
  const source = await tool("opencut_project_v4_inspect", {
    project_id: original.project_id,
    revision: null,
  });
  expect(source.revision).toBe(original.revision);
  const metadata = path.join(directory, "scene-preview-identity-fixture.json");
  let fixture = await fs
    .readFile(metadata, "utf8")
    .then(JSON.parse)
    .catch((e) => {
      if (e.code === "ENOENT") return null;
      throw e;
    });
  if (!fixture) {
    const id = "scene-preview-identity-" + crypto.randomUUID();
    const copied = await tool("opencut_project_v4_copy", {
      source_project_id: source.project.id,
      source_revision: source.revision,
      project_id: id,
      name: "Preview scene and revision boundaries",
      idempotency_key: crypto.randomUUID(),
    });
    const scene = copied.project.sequences.find(
      (s) => s.id === copied.project.default_sequence_id,
    );
    const emptyTrack = {
      ...structuredClone(
        scene.tracks.find((t) => t.id === scene.main_track_id),
      ),
      id: crypto.randomUUID(),
      name: "Alternate empty main track",
      elements: [],
    };
    const alternate = {
      ...structuredClone(scene),
      id: "alternate",
      name: "Alternate empty scene",
      main_track_id: emptyTrack.id,
      tracks: [emptyTrack],
      captions: [],
      bookmarks: [],
      transitions: [],
    };
    const changed = await tool("opencut_edit_v4_apply", {
      project_id: id,
      expected_revision: copied.revision,
      idempotency_key: crypto.randomUUID(),
      label: "Add alternate scene for preview identity acceptance",
      operations: [{ type: "sequence_put", sequence: alternate }],
    });
    fixture = {
      project_id: id,
      revision: changed.revision,
      baseline_revision: copied.revision,
      sequence_id: scene.id,
      alternate_sequence_id: alternate.id,
      source_project_id: source.project.id,
      source_revision: source.revision,
    };
    await fs.writeFile(metadata, JSON.stringify(fixture) + "\n", {
      flag: "wx",
    });
  }
  const fixed = await tool("opencut_project_v4_inspect", {
    project_id: fixture.project_id,
    revision: null,
  });
  expect(fixed.revision).toBe(fixture.revision);
  expect(fixed.project.assets.map((a) => [a.id, a.version])).toEqual(
    source.project.assets.map((a) => [a.id, a.version]),
  );
  return fixture;
}

export async function verifyV4ScenePreviewIdentity(
  browser,
  endpoint,
  directory,
  check,
  { revocationOnly = false } = {},
) {
  const service = JSON.parse(
    await fs.readFile(path.join(directory, "service/state.json"), "utf8"),
  );
  const owner = await browser.newPage({ locale: "en-US" });
  await ready(owner, endpoint);
  const tool = toolFor(owner);
  const original = JSON.parse(
    await fs.readFile(
      path.join(directory, "scene-preview-fixture.json"),
      "utf8",
    ),
  );
  const evidence = [];
  const admin = (args, input) => {
    const result = execFileSync(
      process.env.OPENCUT_WEB_BINARY,
      [
        "--database",
        path.join(directory, "workspace.sqlite"),
        "--json",
        "access",
        ...args,
      ],
      {
        input: input === undefined ? undefined : JSON.stringify(input),
        encoding: "utf8",
        timeout: 30000,
        maxBuffer: 1024 * 1024,
      },
    );
    const reply = JSON.parse(result);
    if (reply.error)
      throw Error(`Acceptance administration failed: ${reply.error.code}`);
    return reply.data;
  };
  try {
    if (!revocationOnly) {
      const fixture = await fixtureFor(tool, directory);
      for (const boundary of [
        "scene_with_accepted_seek",
        "historical_revision",
        "return_to_head",
      ]) {
        const page = await browser.newPage({
          locale: "en-US",
          viewport: { width: 1440, height: 1100 },
        });
        await page.addInitScript(installScenePreviewWitness);
        const witness = trackScenePreviewPumps(page);
        const controls = [],
          writes = [],
          errors = [];
        let overflow = false,
          armed = false,
          held = false,
          release;
        const gate = new Promise((r) => {
          release = r;
        });
        page.on("pageerror", (e) => {
          if (errors.length < 8) errors.push(e.message);
          else overflow = true;
        });
        page.on("request", (r) => {
          if (r.url().endsWith("opencut_edit_v4_apply")) {
            if (writes.length < 8) writes.push(r.url());
            else overflow = true;
          }
        });
        await page
          .context()
          .route(
            "**/v1/tools/opencut_audio_preview_v4_control",
            async (route) => {
              const input = route.request().postDataJSON();
              if (controls.length < 2048) controls.push(input);
              else overflow = true;
              if (armed && !held && input.command.kind === "seek") {
                const response = await route.fetch();
                expect(response.status()).toBe(200);
                held = true;
                await gate;
                await route.fulfill({ response });
              } else await route.continue();
            },
          );
        try {
          await ready(page, endpoint, fixture.project_id);
          if (boundary === "return_to_head") {
            await page
              .locator(".v4-revisions")
              .getByRole("button", { name: "View revision", exact: true })
              .nth(1)
              .click();
            await expect(
              page.getByText("Viewing a saved revision · Read only", {
                exact: false,
              }),
            ).toBeVisible();
          }
          const session = await play(page, witness);
          expect(session.identity).toMatchObject({
            project_id: fixture.project_id,
            sequence_id: fixture.sequence_id,
            revision:
              boundary === "return_to_head"
                ? fixture.baseline_revision
                : fixture.revision,
          });
          if (boundary === "scene_with_accepted_seek") {
            armed = true;
            const field = page.getByLabel("Playhead (seconds)", {
              exact: true,
            });
            await field.fill("3.25");
            await field.press("Enter");
            await expect.poll(() => held).toBe(true);
          }
          const before = snapshot(service.pid);
          expect(before.some((r) => r.role === "node")).toBe(true);
          const owned = new Map(
            before
              .filter((r) => r.pid !== service.pid)
              .map((r) => [r.pid, r.start]),
          );
          if (boundary === "scene_with_accepted_seek") {
            await page
              .getByLabel("V4 scene", { exact: true })
              .selectOption(fixture.alternate_sequence_id);
            release();
            await expect(
              page.getByLabel("V4 scene", { exact: true }),
            ).toHaveValue(fixture.alternate_sequence_id);
          } else if (boundary === "historical_revision") {
            await page
              .locator(".v4-revisions")
              .getByRole("button", { name: "View revision", exact: true })
              .nth(1)
              .click();
            await expect(
              page.getByText("Viewing a saved revision · Read only", {
                exact: false,
              }),
            ).toBeVisible();
          } else
            await page
              .getByRole("button", { name: "Return to current", exact: true })
              .click();
          if (boundary === "historical_revision")
            await expect(
              page.getByLabel("V4 scene", { exact: true }).locator("option"),
            ).toHaveCount(1);
          if (boundary === "return_to_head") {
            await expect(page.locator(".history-banner")).toBeHidden();
            await expect(
              page.getByLabel("V4 scene", { exact: true }).locator("option"),
            ).toHaveCount(2);
          }
          let final;
          await expect
            .poll(
              async () => {
                final = await tool("opencut_audio_preview_v4_inspect", {
                  session_id: session.session_id,
                });
                return final.state;
              },
              { timeout: 5000 },
            )
            .toBe("stopped");
          const cleanup = await exited(browser, page, owned, service.pid);
          expect(final.identity).toEqual(session.identity);
          expect(final.owner_id).toBe(session.owner_id);
          expect(final.lease_id).toBe(session.lease_id);
          const stops = controls.filter(
            (r) =>
              r.session_id === session.session_id && r.command.kind === "stop",
          );
          expect(stops.map((r) => r.generation)).toEqual(
            boundary === "scene_with_accepted_seek"
              ? [session.generation, String(BigInt(session.generation) + 1n)]
              : [session.generation],
          );
          if (boundary === "scene_with_accepted_seek") {
            const seeks = controls.filter(
              (r) =>
                r.session_id === session.session_id &&
                r.command.kind === "seek",
            );
            expect(seeks.length).toBe(2);
            expect(JSON.stringify(seeks[1]) === JSON.stringify(seeks[0])).toBe(
              true,
            );
          }
          expect(
            stops.every(
              (r) =>
                r.owner_id === session.owner_id &&
                r.lease_id === session.lease_id,
            ),
          ).toBe(true);
          expect(controls.filter((r) => r.command.kind === "play").length).toBe(
            1,
          );
          expect(
            (
              await tool("opencut_project_v4_inspect", {
                project_id: fixture.project_id,
                revision: null,
              })
            ).revision,
          ).toBe(fixture.revision);
          expect(writes.length).toBe(0);
          expect(errors).toEqual([]);
          expect(overflow).toBe(false);
          const record = {
            boundary,
            project_id: fixture.project_id,
            fixed_revision: session.identity.revision,
            head_revision: fixture.revision,
            original_authority: true,
            stop_generations: stops.map((r) => r.generation),
            no_implicit_play: true,
            readonly: true,
            ...cleanup,
          };
          evidence.push(record);
          check(
            `actual ${boundary} retires original authority, late accepted generation and owned picture/PCM/native resources`,
            { scene_preview_identity: record },
          );
        } finally {
          release();
          await page
            .context()
            .unroute("**/v1/tools/opencut_audio_preview_v4_control");
          await page.close();
        }
      }
    } else {
      const principal = "preview-revocation-" + crypto.randomUUID();
      admin(["set-policy", "--input", "-"], {
        principal_id: principal,
        expected_revision: "0",
        enabled: true,
        grants: [
          { project_id: original.project_id, actions: ["read", "run_jobs"] },
        ],
      });
      for (const phase of ["playing", "paused"]) {
        const credentialFile = path.join(
          directory,
          `preview-revocation-${phase}-${crypto.randomUUID()}.secret`,
        );
        const issued = admin(["issue", principal, "--output", credentialFile]);
        const secret = (await fs.readFile(credentialFile, "utf8")).trim();
        const context = await browser.newContext({
          locale: "en-US",
          viewport: { width: 1440, height: 1100 },
          extraHTTPHeaders: { Authorization: `Bearer ${secret}` },
        });
        const page = await context.newPage();
        await page.addInitScript(installScenePreviewWitness);
        const witness = trackScenePreviewPumps(page);
        const denials = [],
          errors = [];
        let revoked = false,
          overflow = false;
        page.on("pageerror", (e) => {
          if (errors.length < 8) errors.push(e.message);
          else overflow = true;
        });
        page.on("response", (r) => {
          const pathname = new URL(r.url()).pathname;
          if (pathname.startsWith("/v1/") && [401, 403].includes(r.status())) {
            if (denials.length < 64)
              denials.push({ path: pathname, status: r.status() });
            else overflow = true;
          }
        });
        try {
          await ready(page, endpoint, original.project_id);
          const access = await toolFor(page)("opencut_access_inspect", {});
          expect(access.owner).toBe(false);
          expect(access.principal_id).toBe(principal);
          expect(access.credential_id).toBe(issued.credential_id);
          const session = await play(page, witness);
          expect(session.owner_id).toBe(principal);
          expect(session.identity).toMatchObject({
            project_id: original.project_id,
            revision: original.revision,
          });
          if (phase === "paused") {
            await page
              .getByRole("button", { name: "Pause scene audio", exact: true })
              .click();
            await expect(
              page.getByText("Scene sound paused", { exact: true }),
            ).toBeVisible();
          }
          const before = snapshot(service.pid);
          expect(before.some((r) => r.role === "node")).toBe(true);
          const owned = new Map(
            before
              .filter((r) => r.pid !== service.pid)
              .map((r) => [r.pid, r.start]),
          );
          admin(["revoke", issued.credential_id]);
          revoked = true;
          let final;
          await expect
            .poll(
              async () => {
                final = await tool("opencut_audio_preview_v4_inspect", {
                  session_id: session.session_id,
                });
                return final.state;
              },
              { timeout: 5000 },
            )
            .toBe("failed");
          const cleanup = await exited(browser, page, owned, service.pid);
          const denied = await page.evaluate(async (session) => {
            const response = await fetch(
              "/v1/tools/opencut_visual_preview_v4_frame",
              {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({
                  session: {
                    session_id: session.session_id,
                    generation: session.generation,
                    overlay_epoch: session.overlay_epoch,
                    owner_id: session.owner_id,
                    lease_id: session.lease_id,
                  },
                  time: null,
                  decoded: null,
                }),
              },
            );
            const reply = await response.json();
            return {
              status: response.status,
              code: reply.error?.code,
              data_absent: reply.data == null,
            };
          }, session);
          expect([401, 403].includes(denied.status)).toBe(true);
          expect(denied.code).toBe("NOT_AUTHORIZED");
          expect(denied.data_absent).toBe(true);
          expect(
            denials.some((d) =>
              /opencut_(visual_preview_v4_frame|audio_preview_v4_control)|preview\/v4\/audio\/stream/.test(
                d.path,
              ),
            ),
          ).toBe(true);
          expect(final.owner_id).toBe(session.owner_id);
          expect(final.lease_id).toBe(session.lease_id);
          expect(final.identity).toEqual(session.identity);
          expect(
            (
              await tool("opencut_project_v4_inspect", {
                project_id: original.project_id,
                revision: null,
              })
            ).revision,
          ).toBe(original.revision);
          expect(errors).toEqual([]);
          expect(overflow).toBe(false);
          const record = {
            boundary: `credential_revoke_${phase}`,
            project_id: original.project_id,
            revision: original.revision,
            original_credential_verified: true,
            owner_fallback: false,
            native_state: final.state,
            actual_denials: denials,
            explicit_cached_frame_denied: denied,
            readonly: true,
            ...cleanup,
          };
          evidence.push(record);
          check(
            `actual original credential revocation while ${phase} denies cached picture/PCM and reaps owned native realm`,
            { scene_preview_identity: record },
          );
        } finally {
          if (!revoked) admin(["revoke", issued.credential_id]);
          await context.close();
          await fs.unlink(credentialFile);
        }
      }
    }
    expect(
      (
        await tool("opencut_project_v4_inspect", {
          project_id: original.project_id,
          revision: null,
        })
      ).revision,
    ).toBe(original.revision);
  } finally {
    await owner.close();
    await fs.writeFile(
      path.join(
        directory,
        revocationOnly
          ? "scene-preview-revocation-evidence.json"
          : "scene-preview-identity-evidence.json",
      ),
      JSON.stringify(evidence, null, 2) + "\n",
    );
  }
}
