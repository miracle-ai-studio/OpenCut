/** Built generated client: typed recipe metadata and actual public artifact download. */
import fs from "node:fs/promises";
import { createHash } from "node:crypto";
import { OpenCutClient } from "../packages/sdk-ts/dist/index.js";
const [baseUrl, credential, input, output] = process.argv.slice(2);
const client = new OpenCutClient({
  baseUrl,
  token: (await fs.readFile(credential, "utf8")).trim(),
});
const request = JSON.parse(await fs.readFile(input, "utf8"));
const recipe = await client.catalogSoundRecipe(request.lookup);
const response = await client.downloadArtifact(request.artifact);
const data = Buffer.from(await response.arrayBuffer());
await fs.writeFile(
  output,
  JSON.stringify(
    {
      recipe,
      downloaded_sha256: createHash("sha256").update(data).digest("hex"),
      bytes: data.length,
    },
    null,
    2,
  ) + "\n",
);
