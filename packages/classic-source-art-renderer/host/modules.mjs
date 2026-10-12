import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { moduleRegistry as sharedRegistry, playwrightRuntime as sharedPlaywright, exactDependency, PACKAGE_ROOT as SHARED_ROOT } from "../../classic-audio-renderer/host/shared.mjs";
export const PACKAGE_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
export function dependencyRuntime(runtime) { return { ...runtime,
  mediabunnyRoot: runtime.mediabunnyRoot ?? path.join(SHARED_ROOT, "node_modules/mediabunny"),
  playwrightRoot: runtime.playwrightRoot ?? path.join(SHARED_ROOT, "node_modules/playwright-core") }; }
async function runtimeFiles(root, relative = "") {
  const files = [];
  for (const entry of await fs.readdir(path.join(root, relative), { withFileTypes: true })) {
    const name = path.join(relative, entry.name);
    if (entry.isDirectory()) files.push(...await runtimeFiles(root, name));
    else if (/\.(?:js|mjs|json)$/.test(entry.name)) files.push(name);
    if (files.length > 512) throw new Error("Source-art runtime manifest exceeds file budget");
  }
  return files;
}
export async function moduleRegistry(runtime) {
  runtime = dependencyRuntime(runtime);
  const playwright = await exactDependency("playwright-core", "1.63.0", runtime.playwrightRoot);
  const media = await exactDependency("mediabunny", "1.41.0", runtime.mediabunnyRoot);
  const identityFiles = ["shared", "browser", "runner", "modules", "limits", "protocol", "page"].map(file => ({ root: SHARED_ROOT, prefix: "/shared-host/", relative: `host/${file}.mjs` }));
  identityFiles.push({ root: SHARED_ROOT, prefix: "/shared-host/", relative: "src/validation.mjs" });
  identityFiles.push({ root: media.root, prefix: "/dependencies/mediabunny/", relative: "package.json" });
  for (const relative of await runtimeFiles(playwright.root)) identityFiles.push({ root: playwright.root, prefix: "/dependencies/playwright-core/", relative });
  return sharedRegistry(runtime, { packageRoot: PACKAGE_ROOT, vendorFiles: [], identityFiles, staticImages: true });
}
export const playwrightRuntime = runtime => sharedPlaywright(dependencyRuntime(runtime));
