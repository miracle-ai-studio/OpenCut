import { editorSnapshot } from "./project-inspection.mjs";
/** Real cached acquisition, source audition/download, unknown-response recovery and placement. */
import { chromium, expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
const args = Object.fromEntries(
  process.argv
    .slice(2)
    .reduce(
      (pairs, value, i, all) =>
        i % 2 ? pairs : [...pairs, [value.replace(/^--/, ""), all[i + 1]]],
      [],
    ),
);
if (args.boundary === "stop-waiting") {
  await verifyStoppedAcquisition(args);
  process.exit(0);
}
const { binary, workspace, endpoint, report } = args;
const ready = JSON.parse(await fs.readFile(args.ready, "utf8"));
const checkpoint = path.join(
  workspace,
  "agent-catalog-sounds-browser-checkpoint.json",
);
let state;
try {
  state = JSON.parse(await fs.readFile(checkpoint, "utf8"));
} catch (e) {
  if (e.code !== "ENOENT") throw e;
  state = { stages: {}, requests: {} };
}
const save = async () => {
  await fs.writeFile(
    checkpoint + ".pending",
    JSON.stringify(state, null, 2) + "\n",
  );
  await fs.rename(checkpoint + ".pending", checkpoint);
};
const stage = async (name, action) => {
  if (state.stages[name]) {
    console.log("Reuse " + name);
    return state.stages[name];
  }
  console.log("Begin " + name);
  const result = await action();
  state.stages[name] = result;
  await save();
  console.log("Passed " + name);
  return result;
};
const sha = (value) => createHash("sha256").update(value).digest("hex");
const cli = (tool, input) => {
  const result = JSON.parse(
    execFileSync(
      binary,
      [
        "--json",
        "--database",
        path.join(workspace, "workspace.sqlite"),
        "call",
        tool,
        "--input",
        "-",
      ],
      { input: JSON.stringify(input), encoding: "utf8", timeout: 30000 },
    ),
  );
  expect(result.error).toBeNull();
  return result.data;
};
const inspect = () =>
  editorSnapshot(cli("opencut_project_inspect", {
    project_id: ready.project_id,
    revision: null,
  }));
const browser = await chromium.launch({
  headless: true,
  executablePath: process.env.OPENCUT_CHROME,
});
const page = await browser.newPage({
  locale: "en-US",
  viewport: { width: 1440, height: 1100 },
});
page.setDefaultTimeout(30000);
const errors = [],
  writes = [],
  failures = [];
page.on("pageerror", (e) => errors.push(e.message));
page.on("request", (r) => {
  if (r.url().endsWith("/api/tools/opencut_edit_apply"))
    writes.push(r.postDataJSON());
});
page.on("requestfailed", (r) => {
  if (r.url().includes("/"))
    failures.push({
      path: new URL(r.url()).pathname,
      error: r.failure()?.errorText,
    });
});
const panel = page.getByRole("region", {
  name: "Resource catalog",
  exact: true,
});
const sourcePreview = page.getByRole("dialog", {
  name: "Source media preview",
  exact: true,
});
async function open() {
  await page.goto(`${endpoint}/editor/${encodeURIComponent(ready.project_id)}`);
  await expect(
    page.getByRole("main", { name: "OpenCut editor", exact: true }),
  ).toBeVisible();
  const guide = page.getByRole("dialog", {
    name: "Welcome to OpenCut",
    exact: true,
  });
  if (await guide.isVisible())
    await guide.getByRole("button", { name: "Close", exact: true }).click();
  await expect(
    panel.getByLabel("Search catalog", { exact: true }),
  ).toBeEnabled();
}
function acquired(name) {
  return panel
    .locator("article")
    .filter({ has: page.getByText(name, { exact: true }) })
    .filter({
      has: page.getByRole("button", { name: "Download source", exact: true }),
    });
}
async function audition() {
  await expect(sourcePreview).toBeVisible();
  const audio = sourcePreview.locator("audio");
  await expect(audio).toHaveAttribute("src", /\//);
  await expect
    .poll(() =>
      audio.evaluate((el) => Number.isFinite(el.duration) && el.duration > 0),
    )
    .toBe(true);
  const observation = await audio.evaluate(async (el) => {
    await el.play();
    await new Promise((resolve, reject) => {
      const timer = setTimeout(
        () => reject(Error("Actual source playback did not advance")),
        5000,
      );
      const done = () => {
        clearTimeout(timer);
        resolve();
      };
      el.addEventListener("timeupdate", done, { once: true });
      el.addEventListener("ended", done, { once: true });
    });
    const result = {
      duration: el.duration,
      time: el.currentTime,
      ended: el.ended,
      ready_state: el.readyState,
    };
    el.pause();
    return result;
  });
  expect(observation.time).toBeGreaterThan(0);
  observation.recorded_recipe_text = await sourcePreview
    .locator("dl")
    .allTextContents();
  await sourcePreview
    .getByRole("button", { name: "Close", exact: true })
    .click();
  return observation;
}
async function download(name, asset) {
  const pending = page.waitForEvent("download");
  await acquired(name)
    .getByRole("button", { name: "Download source", exact: true })
    .click();
  const file = await pending;
  const bytes = await fs.readFile(await file.path());
  expect(sha(bytes)).toBe(asset.version.content_hash);
  expect(bytes.length).toBe(asset.version.size);
  return {
    filename: file.suggestedFilename(),
    sha256: sha(bytes),
    bytes: bytes.length,
  };
}
try {
  await open();
  await stage("catalog-category-and-group-search", async () => {
    const all = cli("opencut_catalog_resources_list", {});
    await panel
      .getByRole("combobox", { name: /^Category/ })
      .selectOption("flags");
    await panel.getByLabel("Search catalog", { exact: true }).fill("");
    await expect(panel.locator(":scope > article")).toHaveCount(40);
    await panel
      .getByRole("combobox", { name: /^Category/ })
      .selectOption("sounds");
    await panel.getByLabel("Search catalog", { exact: true }).fill("switch");
    const count = all.entries.filter(
      (e) => e.category === "sounds" && e.name.startsWith("switch"),
    ).length;
    await expect(panel.locator(":scope > article")).toHaveCount(count);
    await panel
      .getByLabel("Search catalog", { exact: true })
      .fill("no-such-sound-acceptance");
    await expect(panel.locator(":scope > article")).toHaveCount(0);
    return {
      resource_categories_switched: true,
      sound_group_search_count: count,
      unknown_search_count: 0,
    };
  });
  const imported = await stage("ui-cached-click1", async () => {
    await panel
      .getByRole("combobox", { name: /^Category/ })
      .selectOption("sounds");
    await panel.getByLabel("Search catalog", { exact: true }).fill("click1");
    const entry = panel
      .locator(":scope > article")
      .filter({ has: page.getByText("click1", { exact: true }) });
    await expect(entry).toHaveCount(1);
    await page.route(
      "**/api/tools/opencut_catalog_resources_import",
      async (route) => {
        const input = route.request().postDataJSON();
        if (state.requests.import) expect(input).toEqual(state.requests.import);
        else {
          state.requests.import = input;
          await save();
        }
        await route.continue();
      },
    );
    await entry
      .getByRole("button", { name: "Acquire and preview", exact: true })
      .click();
    const played = await audition();
    await page.unroute("**/api/tools/opencut_catalog_resources_import");
    const after = inspect(),
      asset = after.project.assets.find(
        (a) => a.id === state.requests.import.asset_id,
      );
    expect(asset?.name).toBe("click1");
    expect(asset.license.identifier).toBe("CC0-1.0");
    const receipt = cli(
      "opencut_catalog_resources_import",
      state.requests.import,
    );
    expect(inspect()).toEqual(after);
    expect(receipt.revision).toBe(after.revision);
    return {
      asset,
      request: state.requests.import,
      receipt,
      audition: played,
      download: await download("click1", asset),
    };
  });
  if (state.requests.generate)
    await stage("read-existing-recorded-recipe", async () => {
      const before = inspect();
      const proof = cli("opencut_catalog_resources_recipe", {
        project_id: ready.project_id,
        revision: before.revision,
        asset_id: state.requests.generate.asset_id,
      });
      expect(proof.recording.recipe).toEqual(state.requests.generate.recipe);
      expect(inspect()).toEqual(before);
      return {
        recording: proof,
        original_generation_reused_without_new_asset: true,
      };
    });
  const generated = await stage(
    "ui-whoosh751-lost-response-original-retry",
    async () => {
      await panel
        .getByRole("combobox", { name: /^Sound recipe/ })
        .selectOption("whoosh");
      await panel.getByLabel("Duration (ms)", { exact: true }).fill("751");
      await panel.getByLabel("Seed", { exact: true }).fill("18");
      await panel.getByLabel("Level (dB)", { exact: true }).fill("-12");
      await page.route(
        "**/api/tools/opencut_catalog_resources_generate",
        async (route) => {
          const request = route.request().postDataJSON();
          if (state.requests.generate751)
            expect(request).toEqual(state.requests.generate751);
          else {
            state.requests.generate751 = request;
            await save();
          }
          if (!state.lost_response751) {
            const response = await route.fetch();
            const body = await response.json();
            expect(body.error).toBeNull();
            state.lost_response751 = {
              receipt: body.data,
              reason:
                "Actual server commit; intentionally drop only this response",
            };
            await save();
            await route.abort("failed");
          } else await route.continue();
        },
      );
      await panel
        .getByRole("button", { name: "Generate and preview", exact: true })
        .click();
      await expect(
        panel.getByRole("button", {
          name: "Retry original acquisition",
          exact: true,
        }),
      ).toBeEnabled();
      await open();
      await expect(
        panel.getByRole("button", {
          name: "Retry original acquisition",
          exact: true,
        }),
      ).toBeEnabled();
      await panel
        .getByRole("button", {
          name: "Retry original acquisition",
          exact: true,
        })
        .click();
      const played = await audition();
      state.recovery_witness751 = {
        audition: played,
        committed_revision: state.lost_response751.receipt.revision,
        original_request: state.requests.generate751,
      };
      await save();
      await page.unroute("**/api/tools/opencut_catalog_resources_generate");
      const after = inspect(),
        asset = after.project.assets.find(
          (a) => a.id === state.requests.generate751.asset_id,
        );
      expect(asset?.name).toBe("Whoosh · 751 ms");
      expect(played.recorded_recipe_text.join(" ")).toContain("751");
      expect(played.recorded_recipe_text.join(" ")).toContain("18");
      expect(played.recorded_recipe_text.join(" ")).toContain("-12");
      expect(asset.origin.generation_id).toBeNull();
      expect(after.revision).toBe(state.lost_response751.receipt.revision);
      expect(
        after.project.assets.filter((a) => a.id === asset.id),
      ).toHaveLength(1);
      expect(
        cli("opencut_catalog_resources_recipe", {
          project_id: ready.project_id,
          revision: after.revision,
          asset_id: asset.id,
        }).recording.recipe,
      ).toEqual(state.requests.generate751.recipe);
      expect(
        cli("opencut_catalog_resources_generate", state.requests.generate751)
          .revision,
      ).toBe(after.revision);
      return {
        asset,
        request: state.requests.generate751,
        receipt: state.lost_response751.receipt,
        audition: played,
        reload_original_retry_no_duplicate: true,
        download: await download(asset.name, asset),
      };
    },
  );
  async function place(stageName, name, asset, start) {
    return stage(stageName, async () => {
      if (state.requests[stageName])
        throw new Error("An original placement request exists without a receipt; inspect that transaction before retrying: " + stageName);
      await acquired(name)
        .getByRole("button", { name: "Place media", exact: true })
        .click();
      const dialog = page.getByRole("dialog", { name: "Place media", exact: true });
      await dialog.getByLabel("Timeline start (seconds)", { exact: true }).fill(start);
      await dialog.getByLabel("Maintain pitch", { exact: true }).setChecked(false);
      const planning = page.waitForResponse((r) =>
        r.url().endsWith("/api/tools/opencut_media_import_plan"),
      );
      void planning.catch(() => {});
      await dialog.getByRole("button", { name: "Review placement", exact: true }).click();
      const review = page.getByRole("dialog", { name: "Review media changes", exact: true });
      await Promise.race([
        review.waitFor({ state: "visible" }),
        dialog.getByRole("alert").waitFor({ state: "visible" }).then(async () => {
          throw new Error("Placement form rejected input: " + await dialog.getByRole("alert").innerText());
        }),
      ]);
      const planned = await (await planning).json();
      expect(planned.error).toBeNull();
      const transaction = planned.data.transaction;
      expect(transaction.operations.some((op) =>
        op.type === "element_put" && op.element.content.asset_id === asset.id,
      )).toBe(true);
      await page.route("**/api/tools/opencut_edit_apply", async (route) => {
        const input = route.request().postDataJSON();
        expect(input).toEqual(transaction);
        state.requests[stageName] = input;
        await save();
        await route.continue();
      });
      const applying = page.waitForResponse((r) =>
        r.url().endsWith("/api/tools/opencut_edit_apply"),
      );
      void applying.catch(() => {});
      await review.getByRole("button", { name: "Apply media changes", exact: true }).click();
      const applied = await (await applying).json();
      expect(applied.error).toBeNull();
      expect(writes.at(-1)).toEqual(transaction);
      await expect(review.getByText("Media changes saved.", { exact: true })).toBeVisible();
      await review.getByRole("button", { name: "Done", exact: true }).click();
      await expect(review).toBeHidden();
      await page.unroute("**/api/tools/opencut_edit_apply");
      return { transaction, receipt: applied.data, reviewed_plan: planned.data,
        evidence_origin: "actual UI planning and apply response" };
    });
  }
  const first = await place("ui-placement-click1", "click1", imported.asset, "0");
  const second = await place("ui-placement-whoosh751", generated.asset.name, generated.asset, "0.249");
  const placement = await stage("actual-ui-placement", async () => {
    const after = inspect();
    expect(cli("opencut_edit_apply", first.transaction)).toEqual(first.receipt);
    expect(cli("opencut_edit_apply", second.transaction)).toEqual(second.receipt);
    expect(inspect()).toEqual(after);
    return { first, second, revision: after.revision };
  });
  const saved = inspect();
  await page.reload();
  await expect(
    page.getByRole("main", { name: "OpenCut editor", exact: true }),
  ).toBeVisible();
  expect(inspect()).toEqual(saved);
  expect(saved.project.assets).toHaveLength(state.requests.generate ? 55 : 54);
  expect(errors).toEqual([]);
  expect(
    failures.every(
      (f) =>
        f.path.endsWith("opencut_catalog_resources_generate") &&
        f.error === "net::ERR_FAILED",
    ),
  ).toBe(true);
  await fs.writeFile(
    report,
    JSON.stringify(
      {
        status: "passed_ui_pending_export",
        binary_sha256: sha(await fs.readFile(binary)),
        native_acquisition_candidate_sha256: ready.binary_sha256,
        project_id: ready.project_id,
        revision: saved.revision,
        imported,
        generated,
        placement,
        unknown_response_recovered: true,
        project_reopen_exact: true,
        page_errors: errors,
        expected_dropped_response_failures: failures,
        stages: Object.keys(state.stages),
        scope:
          "Actual fixed cached sound acquisition, source playback/download, generated recipe lost-response and original-request recovery after reload, public UI placement/replay/reopen; no physical audio or export claim yet.",
      },
      null,
      2,
    ) + "\n",
  );
  console.log(
    "Passed UI acquire/audition/download, original-response recovery, placement and reopen",
  );
} catch (error) {
  state.last_failure = { message: String(error), at: new Date().toISOString(), url: page.url(), page_errors: errors, failed_requests: failures };
  console.error(JSON.stringify(state.last_failure, null, 2));
  await save();
  await fs.writeFile(path.join(workspace, "catalog-sounds-browser-failure.html"), await page.content());
  await page.screenshot({ path: path.join(workspace, "catalog-sounds-browser-failure.png"), fullPage: true });
  throw error;
} finally {
  await browser.close();
}

/** The public button stops the waiter, preserving an unknown original command. */
async function verifyStoppedAcquisition(options) {
  const project = "agent-sound-catalog-v4-acceptance";
  const checkpoint = path.join(options.workspace, "agent-catalog-stop-waiting-v1-checkpoint.json");
  const candidate = JSON.parse(await fs.readFile(options.candidate, "utf8"));
  const runtime = JSON.parse(await fs.readFile(path.join(options.workspace, "service/state.json"), "utf8"));
  expect(await fs.realpath(runtime.executable)).toBe(await fs.realpath(options.binary));
  expect(await fs.realpath(runtime.web_root)).toBe(await fs.realpath(candidate.web_root));
  const digest = bytes => createHash("sha256").update(bytes).digest("hex");
  expect(digest(await fs.readFile(options.binary))).toBe(candidate.binary_sha256);
  const state = await fs.readFile(checkpoint, "utf8").then(JSON.parse).catch(error => {
    if (error.code !== "ENOENT") throw error;
    return { candidate: options.candidate, stages: {}, requests: {} };
  });
  expect(state.candidate).toBe(options.candidate);
  const save = async () => {
    await fs.writeFile(checkpoint + ".pending", JSON.stringify(state, null, 2) + "\n");
    await fs.rename(checkpoint + ".pending", checkpoint);
  };
  const stage = async (name, action) => {
    if (state.stages[name]) { console.log("Reuse " + name); return state.stages[name]; }
    console.log("Begin " + name);
    const value = await action(); state.stages[name] = value; await save();
    console.log("Passed " + name); return value;
  };
  const cli = (name, input) => {
    const reply = JSON.parse(execFileSync(options.binary, ["--json", "--database", path.join(options.workspace, "workspace.sqlite"), "call", name, "--input", "-"], { input: JSON.stringify(input), encoding: "utf8", timeout: 30000 }));
    expect(reply.error).toBeNull(); return reply.data;
  };
  const inspect = () => editorSnapshot(cli("opencut_project_inspect", { project_id: project, revision: null }));
  const sql = (query, values = []) => JSON.parse(execFileSync("python3", ["-c", "import sqlite3,json,sys; c=sqlite3.connect('file:'+sys.argv[1]+'?mode=ro',uri=True); print(json.dumps(c.execute(sys.argv[2],json.loads(sys.argv[3])).fetchall()))", path.join(options.workspace, "workspace.sqlite"), query, JSON.stringify(values)], { encoding: "utf8" }));
  const privateGraph = revision => JSON.parse(sql("SELECT document_json FROM revisions WHERE revision=?", [revision])[0][0]);
  const jobs = () => Object.fromEntries(sql("SELECT state,COUNT(*) FROM jobs GROUP BY state"));
  const baseline = await stage("baseline", async () => {
    const snapshot = inspect(), counts = jobs(); expect(counts.queued ?? 0).toBe(0); expect(counts.running ?? 0).toBe(0);
    return { snapshot, private: privateGraph(snapshot.revision), jobs: counts };
  });
  const browser = await chromium.launch({ headless: true, executablePath: process.env.OPENCUT_CHROME });
  const page = await browser.newPage({ locale: "en-US", viewport: { width: 1440, height: 1100 } });
  page.setDefaultTimeout(30000);
  const pageErrors = [], requests = [], failures = [];
  page.on("pageerror", error => pageErrors.push(error.message));
  const importPath = "/api/tools/opencut_catalog_resources_import";
  page.on("request", request => { if (new URL(request.url()).pathname === importPath) requests.push(request.postDataJSON()); });
  page.on("requestfailed", request => { if (new URL(request.url()).pathname === importPath) failures.push(request.failure()?.errorText); });
  const panel = page.getByRole("region", { name: "Resource catalog", exact: true });
  const open = async () => {
    await page.goto(`${options.endpoint}/editor/${project}`);
    const guide = page.getByRole("dialog", { name: "Welcome to OpenCut", exact: true });
    if (await guide.isVisible()) await guide.getByRole("button", { name: "Close", exact: true }).click();
    await expect(panel.getByLabel("Search catalog", { exact: true })).toBeEnabled();
    await expect(page.locator(".save-status")).toHaveText("Saved");
  };
  let release;
  try {
    const recovered = await stage("stop-reload-original-recovery", async () => {
      // An interrupted observation must resolve the retained command; never
      // issue a fresh UI key merely because the response was not observed.
      const resuming = !!state.requests.original;
      if (resuming) {
        expect(typeof state.storageRaw, "Retain the authenticated original envelope; never manufacture another key").toBe("string");
        await page.addInitScript(({ origin, raw }) => {
          if (location.origin === origin) {
            const saved = JSON.parse(raw), historical = Object.hasOwn(saved, "version");
            if (historical ? saved.version !== 1 || Object.keys(saved).sort().join() !== "entries,version" : Object.keys(saved).join() !== "entries") throw Error("Invalid retained catalog recovery envelope");
            sessionStorage.setItem(historical ? "opencut.catalog-resources.v1" : "opencut.catalog-resources", raw);
          }
        }, { origin: new URL(options.endpoint).origin, raw: state.storageRaw });
      }
      {
        await open();
        const held = new Promise(resolve => { release = resolve; });
        let routeFinished;
        const finished = new Promise(resolve => { routeFinished = resolve; });
        await page.route("**" + importPath, async route => {
          try {
            const input = route.request().postDataJSON();
            expect(state.requests.original ?? input).toEqual(input);
            state.requests.original = input;
            state.storageRaw = await page.evaluate(() => sessionStorage.getItem("opencut.catalog-resources"));
            expect(typeof state.storageRaw).toBe("string"); await save();
            const reply = await (await route.fetch()).json();
            expect(reply.error).toBeNull(); state.receipt = reply.data; await save();
            await held;
            await route.abort("failed").catch(() => {});
          } finally { routeFinished(); }
        }, { times: 1 });
        if (resuming) await panel.getByRole("button", { name: "Retry original acquisition", exact: true }).click();
        else {
          await panel.getByLabel("Search catalog", { exact: true }).fill("click3");
          const entry = panel.locator("article").filter({ has: page.getByText("click3", { exact: true }) }).filter({ has: page.getByRole("button", { name: "Acquire and preview", exact: true }) });
          await entry.getByRole("button", { name: "Acquire and preview", exact: true }).click();
        }
        await expect.poll(() => !!state.receipt, { timeout: 30000 }).toBe(true);
        await panel.getByRole("button", { name: "Stop waiting", exact: true }).click();
        await expect(panel.getByRole("button", { name: "Retry original acquisition", exact: true })).toBeEnabled();
        state.stop_observed = true; await save(); release(); await finished;
        await page.reload();
      }
      await expect(panel.getByRole("button", { name: "Retry original acquisition", exact: true })).toBeEnabled();
      const response = page.waitForResponse(reply => new URL(reply.url()).pathname === importPath && reply.request().postDataJSON().idempotency_key === state.requests.original.idempotency_key);
      await panel.getByRole("button", { name: "Retry original acquisition", exact: true }).click();
      const reply = await (await response).json(); expect(reply.error).toBeNull(); expect(reply.data).toEqual(state.receipt);
      await expect(panel.getByRole("button", { name: "Retry original acquisition", exact: true })).toBeHidden();
      const preview = page.getByRole("dialog", { name: "Source media preview", exact: true });
      await expect(preview).toBeVisible(); await preview.getByRole("button", { name: "Close", exact: true }).click();
      expect(requests).toHaveLength(2); expect(requests.every(input => JSON.stringify(input) === JSON.stringify(state.requests.original))).toBe(true);
      const current = inspect(); expect(current.project.assets).toHaveLength(baseline.snapshot.project.assets.length + 1);
      const asset = current.project.assets.find(item => item.id === state.requests.original.asset_id); expect(asset).toBeDefined();
      expect(jobs()).toEqual(baseline.jobs);
      return { original: state.requests.original, receipt: state.receipt, asset, stopped_unknown_wait: true, exact_requests: requests, page_errors: pageErrors, expected_aborted_requests: failures };
    });
    const restored = await stage("restore-complete-project", async () => {
      const input = state.requests.undo ?? { project_id: project, expected_revision: inspect().revision, target_transaction_id: recovered.receipt.transaction_id, idempotency_key: "catalog-stop-waiting-v1-restore" };
      state.requests.undo = input; await save();
      expect(cli("opencut_edit_undo_plan", input).conflicts).toEqual([]);
      const receipt = cli("opencut_edit_undo_apply", input), current = inspect();
      expect(current.project).toEqual(baseline.snapshot.project); expect(privateGraph(current.revision)).toEqual(baseline.private);
      expect(cli("opencut_catalog_resources_import", recovered.original)).toEqual(recovered.receipt);
      expect(inspect()).toEqual(current); expect(jobs()).toEqual(baseline.jobs);
      return { revision: current.revision, public_and_private_graph_equal: true, receipt, replay_after_undo_has_no_effect: true };
    });
    expect(pageErrors).toEqual([]);
    const report = { status: "passed_scoped_ui_stop_waiting_reload_original_acquisition_recovery", candidate: options.candidate, checkpoint, project_id: project, recovery: recovered, restoration: restored, job_counts_unchanged: jobs(), scope: "Actual existing-byte acquisition commits while its response is held; Stop waiting aborts the UI wait, reload retains original intent, retry returns same receipt and sole asset, Undo restores full graph. Not Native cancellation, genuine origin404/429 or new source/decoder/export acceptance." };
    await fs.writeFile(options.report, JSON.stringify(report, null, 2) + "\n"); console.log(report.status);
  } catch (error) {
    state.last_failure = String(error.stack ?? error); await save();
    await fs.writeFile(options.report, JSON.stringify({ status: "failed", checkpoint, error: state.last_failure, page_errors: pageErrors }, null, 2) + "\n");
    await page.screenshot({ path: options.report + ".png" }).catch(() => {}); throw error;
  } finally { release?.(); await browser.close(); }
}
