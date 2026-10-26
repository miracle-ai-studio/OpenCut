import fs from "node:fs";
import path from "node:path";
import os from "node:os";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import {
  digest,
  dataDirectory,
  verifyRuntime,
  runtimeEnvironment,
} from "./runtime.mjs";

const source = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
try {
  if (process.argv.length > 2)
    throw new Error(
      "Configure OPENCUT_PLUGIN_INSTALL_DIR and OPENCUT_PLUGIN_DATA_DIR rather than unknown command options",
    );
  const release = JSON.parse(
    fs.readFileSync(path.join(source, "release.json"), "utf8"),
  );
  const archive = path.join(source, "runtime.tar.gz");
  if (digest(archive) !== release.runtime_archive_sha256)
    throw new Error("Runtime archive checksum differs");
  const prefix = path.resolve(
    process.env.OPENCUT_PLUGIN_INSTALL_DIR ||
      path.join(os.homedir(), "Library/Application Support/OpenCut Plugin"),
  );
  const destination = path.join(prefix, release.version);
  fs.mkdirSync(prefix, { recursive: true, mode: 0o700 });
  if (!fs.existsSync(destination)) {
    const stage = fs.mkdtempSync(path.join(prefix, ".install-"));
    try {
      const unpack = path.join(stage, "unpack");
      fs.mkdirSync(unpack);
      const listing = spawnSync("tar", ["-tzf", archive], {
        encoding: "utf8",
        maxBuffer: 16 * 1024 * 1024,
      });
      if (
        listing.status !== 0 ||
        listing.stdout
          .split("\n")
          .filter(Boolean)
          .some(
            (name) =>
              !name.startsWith(release.runtime_top_dir + "/") ||
              name.split("/").includes(".."),
          )
      )
        throw new Error("Invalid runtime archive paths");
      const extracted = spawnSync("tar", ["-xzf", archive, "-C", unpack], {
        encoding: "utf8",
        maxBuffer: 1024 * 1024,
      });
      if (extracted.status !== 0) throw new Error("Runtime extraction failed");
      fs.renameSync(
        path.join(unpack, release.runtime_top_dir),
        path.join(stage, "runtime"),
      );
      fs.rmSync(unpack, { recursive: true });
      verifyRuntime(path.join(stage, "runtime"));
      await runtimeEnvironment(path.join(stage, "runtime"));
      const plugin = path.join(stage, "plugins/opencut");
      fs.mkdirSync(plugin, { recursive: true });
      for (const name of [
        "plugin.json",
        "mcp.json",
        "assets",
        "skills",
        "scripts",
        ".codex-plugin",
        ".mcp.json",
        "release.json",
        "LICENSE",
        "README.md",
        "README.zh-CN.md",
        "docs",
      ])
        if (fs.existsSync(path.join(source, name)))
          fs.cpSync(path.join(source, name), path.join(plugin, name), {
            recursive: true,
          });
      const marketplace = path.join(stage, ".agents/plugins");
      fs.mkdirSync(marketplace, { recursive: true });
      fs.writeFileSync(
        path.join(marketplace, "marketplace.json"),
        JSON.stringify(
          {
            name: "opencut-plugin",
            interface: { displayName: "OpenCut Plugin" },
            plugins: [
              {
                name: "opencut",
                source: { source: "local", path: "./plugins/opencut" },
                policy: {
                  installation: "AVAILABLE",
                  authentication: "ON_INSTALL",
                },
                category: "Creativity",
              },
            ],
          },
          null,
          2,
        ),
      );
      fs.renameSync(stage, destination);
    } catch (error) {
      fs.rmSync(stage, { recursive: true, force: true });
      throw error;
    }
  }
  const runtime = path.join(destination, "runtime");
  const manifest = verifyRuntime(runtime);
  await runtimeEnvironment(runtime);
  if (manifest.binary_sha256 !== release.binary_sha256)
    throw new Error(
      "Existing installation belongs to another runtime candidate",
    );
  const data = dataDirectory();
  fs.mkdirSync(data, { recursive: true, mode: 0o700 });
  const pending = path.join(data, `install.json.${process.pid}.tmp`);
  fs.writeFileSync(
    pending,
    JSON.stringify(
      {
        format: "opencut.plugin-install.v1",
        runtime,
        version: release.version,
      },
      null,
      2,
    ),
    { mode: 0o600 },
  );
  fs.renameSync(pending, path.join(data, "install.json"));
  console.log(
    JSON.stringify(
      {
        installed: true,
        runtime,
        database: path.join(data, "workspace.sqlite"),
        marketplace: destination,
        command: "codex plugin marketplace add " + JSON.stringify(destination),
        next: "Install opencut@opencut-plugin in the desktop Plugins page or with codex plugin add. Restart the client after installation. No client configuration was changed automatically.",
      },
      null,
      2,
    ),
  );
} catch (error) {
  console.error(`OpenCut Plugin install: ${error.message}`);
  process.exitCode = 1;
}
