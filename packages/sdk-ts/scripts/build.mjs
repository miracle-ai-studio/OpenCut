import { rmSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const root = new URL("../", import.meta.url);
// Deleted source modules must not survive in the published package.
rmSync(new URL("dist/", root), { recursive: true, force: true });
execFileSync(process.execPath, [
  fileURLToPath(new URL("node_modules/typescript/bin/tsc", root)),
  "-p", "tsconfig.json",
], { cwd: fileURLToPath(root), stdio: "inherit" });
