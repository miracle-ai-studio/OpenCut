/** Actual generated SDK consumers; the creation call replays the original request. */
import fs from "node:fs/promises";
import { OpenCutClient } from "../packages/sdk-ts/dist/index.js";
const [baseUrl, credential, input, output] = process.argv.slice(2);
const client = new OpenCutClient({
  baseUrl,
  token: (await fs.readFile(credential, "utf8")).trim(),
});
const request = JSON.parse(await fs.readFile(input, "utf8"));
const created = await client.createReadOnlyProject(request.create);
const view = await client.inspectReadOnlyProject(request.lookup);
const manifest = await client.readOnlyProjectManifest(request.lookup);
await fs.writeFile(
  output,
  JSON.stringify({ created, view, manifest }, null, 2) + "\n",
);
