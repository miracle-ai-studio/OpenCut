// Runs from the isolated Rust acceptance directory against its installed TS package.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { OpenCutClient } from "@opencut/sdk";

const [baseUrl, tokenFile, fixtureFile, rustReport] = process.argv.slice(2);
const client = new OpenCutClient({ baseUrl, token: readFileSync(tokenFile, "utf8").trim() });
const fixture = JSON.parse(readFileSync(fixtureFile, "utf8"));
const rust = JSON.parse(readFileSync(rustReport, "utf8"));
const original = await client.call("opencut_project_create", fixture.create);
assert.equal(original.revision, rust.transaction.base_revision);
const repeated = await client.call("opencut_edit_apply", {
  project_id: fixture.create.project.id,
  ...fixture.request, expected_revision: original.revision, operations: fixture.operations,
});
assert.deepEqual(repeated, rust.transaction);
assert.deepEqual(await client.call("opencut_project_inspect", { project_id: fixture.create.project.id }), rust.snapshot);
const events = await client.call("opencut_event_changes", { after_sequence: 0 });
assert.equal(events.filter(event => event.revision === repeated.revision).length, 1);
console.log("TypeScript replays the Rust SDK transaction with the same fixture and no new commit.");
