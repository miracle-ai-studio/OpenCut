/** Exercise the built typed SDK against the same retained canonical plugin proposal. */
import fs from "node:fs/promises";
import { OpenCutClient } from "../packages/sdk-ts/dist/index.js";
const [baseUrl, credential, request, output] = process.argv.slice(2);
const client = new OpenCutClient({ baseUrl, token: (await fs.readFile(credential, "utf8")).trim() });
const result = await client.planProjectPlugin(JSON.parse(await fs.readFile(request, "utf8")));
await fs.writeFile(output, JSON.stringify(result, null, 2) + "\n");
