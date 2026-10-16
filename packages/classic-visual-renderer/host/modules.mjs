import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { createHash } from "node:crypto";
import { moduleRegistry as sharedRegistry, playwrightRuntime as sharedPlaywright, PACKAGE_ROOT as SHARED_ROOT } from "@opencut/classic-audio-renderer/internal-host-runtime";
export const PACKAGE_ROOT=path.resolve(path.dirname(fileURLToPath(import.meta.url)),"..");
export const SHADERS=["vendor/classic-v31/fullscreen.wgsl","vendor/classic-v31/layer.wgsl","vendor/classic-v31/blend.wgsl","vendor/classic-v31/mask.wgsl","vendor/classic-v31/jfa_init.wgsl","vendor/classic-v31/jfa_step.wgsl","vendor/classic-v31/jfa_distance.wgsl","vendor/classic-v31/gaussian_blur.wgsl"];
export async function moduleRegistry(runtime) {
  const identityFiles=[];
  for(const file of await fs.readdir(path.join(SHARED_ROOT,"host")))if(file.endsWith(".mjs"))identityFiles.push({root:SHARED_ROOT,prefix:"/shared-host/",relative:"host/"+file});
  identityFiles.push({root:SHARED_ROOT,prefix:"/shared-host/",relative:"src/validation.mjs"});
  const modules=await sharedRegistry(runtime,{packageRoot:PACKAGE_ROOT,vendorFiles:SHADERS,identityFiles,staticImages:true});
  const shaderDigest=createHash("sha256");
  // Hash actual captured source, not a second read that can race the registry.
  for(const name of [...SHADERS].sort()) {const wrapped=modules.registry.get("/renderer/"+name).toString("utf8");const source=JSON.parse(wrapped.slice("export default ".length,-1));shaderDigest.update(name);shaderDigest.update("\0");shaderDigest.update(source);shaderDigest.update("\0");}
  modules.shaderSha256=shaderDigest.digest("hex");return modules;
}
export const playwrightRuntime=runtime=>sharedPlaywright(runtime,PACKAGE_ROOT);
