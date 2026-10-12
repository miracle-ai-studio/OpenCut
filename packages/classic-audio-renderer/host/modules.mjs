import fs from "node:fs/promises";
import { createReadStream } from "node:fs";
import path from "node:path";
import { createHash, randomBytes } from "node:crypto";
import { fileURLToPath, pathToFileURL } from "node:url";
import { integer, reject } from "../src/validation.mjs";

export const PACKAGE_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
export const ORIGIN = "http://127.0.0.1:49199";
const MODULE_MAX = 8 * 1024 * 1024, REGISTRY_MAX = 64 * 1024 * 1024;
export async function exactDependency(name, version, configuredRoot, packageRoot = PACKAGE_ROOT) {
  if (configuredRoot !== undefined && (typeof configuredRoot !== "string" || !path.isAbsolute(configuredRoot))) reject("INVALID_AUDIO_RUNTIME", "Dependency roots must be absolute host paths");
  const root = await fs.realpath(configuredRoot ?? path.join(packageRoot, "node_modules", name));
  const metadataPath = path.join(root, "package.json");
  integer((await fs.stat(metadataPath)).size, "dependency metadata bytes", 1, 65536);
  const handle = await fs.open(metadataPath, "r"); let captured;
  try { const buffer = Buffer.alloc(65537); const { bytesRead } = await handle.read(buffer, 0, buffer.length, 0); integer(bytesRead, "dependency metadata bytes", 1, 65536); captured = buffer.subarray(0, bytesRead); }
  finally { await handle.close(); }
  const metadata = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(captured));
  if (metadata.name !== name || metadata.version !== version) reject("AUDIO_RUNTIME_VERSION_MISMATCH", "Installed browser dependency does not match its exact pin");
  return { root, metadata };
}
export async function moduleRegistry(runtime, options = {}) {
  const packageRoot = options.packageRoot ?? PACKAGE_ROOT;
  const registry = new Map(), identityModules = new Map(); let total = 0;
  async function add(root, prefix, relative, served = true) {
    const filename = await fs.realpath(path.join(root, relative));
    if (!filename.startsWith(root + path.sep)) reject("INVALID_AUDIO_RUNTIME", "Module symlinks cannot escape their fixed package root");
    const stat = await fs.stat(filename);
    integer(stat.size, "module bytes", 1, MODULE_MAX);
    if (stat.size > REGISTRY_MAX - total || identityModules.size >= 1024) reject("AUDIO_RUNTIME_BUDGET", "Fixed module registry exceeds its byte or file budget");
    const limit = Math.min(MODULE_MAX, REGISTRY_MAX - total), chunks = []; let length = 0;
    // Bound the read even if an admitted source file grows after stat().
    for await (const chunk of createReadStream(filename, { end: limit })) {
      length += chunk.length;
      if (length > limit) reject("AUDIO_RUNTIME_BUDGET", "Fixed module source exceeds its admitted byte budget");
      chunks.push(chunk);
    }
    integer(length, "module bytes", 1, MODULE_MAX);
    const bytes = Buffer.concat(chunks, length), name = prefix + relative.split(path.sep).join("/"); total += bytes.length;
    identityModules.set(name, bytes);
    if (served) registry.set(name, bytes);
  }
  for (const file of await fs.readdir(path.join(packageRoot, "src"))) {
    if (file.endsWith(".mjs")) await add(packageRoot, "/renderer/", path.join("src", file));
  }
  for (const file of options.vendorFiles ?? ["vendor/soundtouchjs-0.3.0/soundtouch.js"]) {
    await add(packageRoot, "/renderer/", file);
    if (file.endsWith(".wgsl")) registry.set("/renderer/" + file, Buffer.from("export default " + JSON.stringify(registry.get("/renderer/" + file).toString("utf8")) + ";"));
  }
  // Node control code and the serialized page function affect execution, but
  // must never become browser-served modules or expose their private imports.
  for (const file of await fs.readdir(path.join(packageRoot, "host"))) {
    if (file.endsWith(".mjs")) await add(packageRoot, "/renderer/", path.join("host", file), false);
  }
  for (const extra of options.identityFiles ?? []) await add(extra.root, extra.prefix, extra.relative, false);
  const media = await exactDependency("mediabunny", "1.41.0", runtime.mediabunnyRoot, packageRoot);
  // The official browser bundle applies mediabunny's browser aliases, disabling
  // Node filesystem code. Its source ESM entry statically imports node:fs.
  const entry = "dist/bundles/mediabunny.mjs";
  await add(media.root, "/dependencies/mediabunny/", entry);
  const entryUrl = "/dependencies/mediabunny/" + entry;
  if (!registry.has(entryUrl)) reject("INVALID_AUDIO_RUNTIME", "Pinned mediabunny ESM entry was not captured");
  const digest = createHash("sha256");
  for (const [name, bytes] of [...identityModules].sort(([a], [b]) => a.localeCompare(b))) { digest.update(name); digest.update("\0"); digest.update(bytes); digest.update("\0"); }
  const moduleSha256 = digest.digest("hex"), nonce = randomBytes(16).toString("hex");
  const csp = `default-src 'none'; script-src 'self' 'nonce-${nonce}'; connect-src 'none'; media-src blob:; ${options.staticImages ? 'img-src blob:;' : ''} worker-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'`;
  const html = `<!doctype html><meta charset="utf-8"><title>OpenCut isolated audio</title><script type="importmap" nonce="${nonce}">${JSON.stringify({ imports: { mediabunny: entryUrl } })}</script>`;
  return { registry, moduleSha256, html, csp, dependencyBytes: total };
}
export async function playwrightRuntime(runtime, packageRoot = PACKAGE_ROOT) {
  const dependency = await exactDependency("playwright-core", "1.63.0", runtime.playwrightRoot, packageRoot);
  return import(pathToFileURL(path.join(dependency.root, "index.mjs")).href);
}

export function moduleResponse(modules, address, method) {
  let url;
  try { url = new URL(address); } catch { return undefined; }
  if (url.origin !== ORIGIN || url.search || method !== "GET") return undefined;
  if (url.pathname === "/") return { contentType: "text/html", body: modules.html, headers: { "Content-Security-Policy": modules.csp, "Cache-Control": "no-store" } };
  const bytes = modules.registry.get(url.pathname);
  if (!bytes) return undefined;
  return { contentType: "text/javascript", body: bytes, headers: { "Cache-Control": "no-store", "X-Content-Type-Options": "nosniff" } };
}
