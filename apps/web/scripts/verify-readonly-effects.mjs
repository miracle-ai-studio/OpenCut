import { preservedProject } from "./project-inspection.mjs";
/** Actual readonly open/reload/download and server-side export refusal. No media jobs. */
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
const { binary, workspace, endpoint, report } = args;
const ready = JSON.parse(await fs.readFile(args.ready, "utf8"));
const sha = (bytes) => createHash("sha256").update(bytes).digest("hex");
const files = [];
async function visit(relative = "") {
  for (const entry of await fs.readdir(path.join("dist", relative), {
    withFileTypes: true,
  })) {
    const name = path.join(relative, entry.name);
    if (entry.isDirectory()) await visit(name);
    else if (entry.isFile() && /\.(js|css|mjs|cjs)$/u.test(name))
      files.push([name, sha(await fs.readFile(path.join("dist", name)))]);
  }
}
await visit();
files.sort(([a], [b]) => a.localeCompare(b));
const candidate = {
  binary_sha256: sha(await fs.readFile(binary)),
  web_js_css_tree_sha256: sha(JSON.stringify(files)),
};
expect(candidate.binary_sha256).toBe(ready.binary_sha256);
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
const browser = await chromium.launch({
  headless: true,
  executablePath: process.env.OPENCUT_CHROME,
});
const page = await browser.newPage({
  locale: "en-US",
  viewport: { width: 1440, height: 1000 },
});
page.setDefaultTimeout(30000);
const errors = [];
page.on("pageerror", (error) => errors.push(error.message));
const requests = [],
  failures = [];
page.on("request", (request) => {
  if (request.url().includes("/api/tools/"))
    requests.push({
      path: new URL(request.url()).pathname,
      input: request.postDataJSON(),
    });
});
page.on("requestfailed", (request) => {
  if (request.url().includes("/api/tools/"))
    failures.push({
      path: new URL(request.url()).pathname,
      error: request.failure()?.errorText,
    });
});
try {
  await page.goto(`${endpoint}/editor/${encodeURIComponent(ready.project_id)}`);
  const main = page.getByRole("main", {
    name: "Read-only project",
    exact: true,
  });
  await expect(main).toBeVisible();
  const guide = page.getByRole("dialog", {
    name: "Welcome to OpenCut",
    exact: true,
  });
  if (await guide.isVisible())
    await guide.getByRole("button", { name: "Close", exact: true }).click();
  await expect(main).toHaveAttribute("data-revision", ready.revision);
  await expect(
    main.getByText("Plugin is not installed", { exact: true }),
  ).toBeVisible();
  await expect(
    main.locator('[data-effect-id="preserved-unavailable-effect"]'),
  ).toHaveAttribute("data-payload-sha256", ready.payload_sha256);
  expect(
    await page.getByRole("main", { name: "OpenCut editor", exact: true }).count(),
  ).toBe(0);
  expect(
    await main
      .locator("input,textarea,[contenteditable=true],canvas,video")
      .count(),
  ).toBe(0);
  const downloadPromise = page.waitForEvent("download");
  await main
    .getByRole("button", { name: "Download project data", exact: true })
    .click();
  const download = await downloadPromise;
  const destination = path.join(
    workspace,
    "agent-readonly-effects-downloaded.json",
  );
  await download.saveAs(destination);
  const manifest = JSON.parse(await fs.readFile(destination, "utf8"));
  expect(manifest).toEqual(ready.manifest);
  const refusal = page.waitForResponse((response) =>
    response.url().endsWith("/api/tools/opencut_render_submit"),
  );
  await main.getByRole("button", { name: "Export video", exact: true }).click();
  const response = await refusal;
  expect((await response.json()).error.code).toBe("MISSING_PLUGIN_DEPENDENCY");
  await expect(main.getByRole("alert")).toHaveText(
    "Export is blocked because required effect plugins are unavailable. Your project data is preserved.",
  );
  expect(preservedProject(cli("opencut_project_inspect", ready.lookup))).toEqual(
    ready.view,
  );
  await page.screenshot({
    path: path.join(workspace, "agent-readonly-effects.png"),
    fullPage: true,
  });
  await page.reload();
  await expect(main).toBeVisible();
  await expect(main).toHaveAttribute("data-revision", ready.revision);
  await expect(
    main.getByText("Plugin is not installed", { exact: true }),
  ).toBeVisible();
  expect(cli("opencut_project_readonly_manifest", ready.lookup)).toEqual(
    ready.manifest,
  );
  expect(errors).toEqual([]);
  expect(failures).toEqual([]);
  expect(
    requests.filter((r) =>
      /opencut_(edit|analysis|preview|audio_preview)/u.test(r.path),
    ),
  ).toEqual([]);
  await fs.writeFile(
    report,
    JSON.stringify(
      {
        status: "passed_scoped",
        candidate,
        project_id: ready.project_id,
        revision: ready.revision,
        readonly_open_and_reload: true,
        absent_edit_and_preview_surfaces: true,
        downloaded_manifest_exact: true,
        formal_export_error: "MISSING_PLUGIN_DEPENDENCY",
        screenshot: path.join(workspace, "agent-readonly-effects.png"),
        errors,
        failures,
        requests,
      },
      null,
      2,
    ) + "\n",
  );
  console.log(
    "Passed readonly open, exact download, actual export refusal and reload",
  );
} finally {
  await browser.close();
}
