import { readConfig, verifyRuntime, runtimeEnvironment } from "./runtime.mjs";
try {
  const config = readConfig();
  const manifest = verifyRuntime(config.runtime);
  await runtimeEnvironment(config.runtime);
  console.log(
    JSON.stringify(
      {
        ready: true,
        target: manifest.target,
        binary_sha256: manifest.binary_sha256,
        scope:
          "File integrity and base CLI/MCP/media prerequisites. Optional font derivation and end-to-end rendering require their own checks.",
      },
      null,
      2,
    ),
  );
} catch (error) {
  console.error(JSON.stringify({ ready: false, error: error.message }));
  process.exitCode = 1;
}
