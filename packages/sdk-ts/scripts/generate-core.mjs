import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";


export async function generateContracts(compile, args = process.argv.slice(2)) {
const root = resolve(import.meta.dirname, "../../..");
const target = resolve(root, "packages/sdk-ts/src/contracts.generated.ts");
const argumentsSet = new Set(args);
const binaryIndex = args.indexOf("--binary");
const suppliedBinary = binaryIndex >= 0 ? args[binaryIndex + 1] : undefined;
if (binaryIndex >= 0 && (!suppliedBinary || suppliedBinary.startsWith("--"))) {
  throw new Error("--binary requires a path");
}
const binary = suppliedBinary ? resolve(suppliedBinary) : resolve(
  root,
  "target/debug",
  process.platform === "win32" ? "opencut.exe" : "opencut",
);
if (binaryIndex >= 0) {
  argumentsSet.delete("--binary");
  argumentsSet.delete(suppliedBinary);
}
const supportedArguments = new Set(["--check", "--skip-build"]);
const unknownArguments = [...argumentsSet].filter(
  (argument) => !supportedArguments.has(argument),
);
if (unknownArguments.length > 0) {
  throw new Error(`Unknown argument(s): ${unknownArguments.join(", ")}`);
}
const check = argumentsSet.has("--check");
const skipBuild = argumentsSet.has("--skip-build");

if (!skipBuild && !suppliedBinary) {
  execFileSync("cargo", ["build", "-p", "opencut-cli"], {
    cwd: root,
    stdio: "inherit",
  });
}

const envelope = JSON.parse(
  execFileSync(binary, ["system", "schema", "--json"], { encoding: "utf8" }),
);
if (envelope.error || !Array.isArray(envelope.data))
  throw new Error(envelope.error?.message ?? "Invalid schema registry");
const capabilitiesEnvelope = JSON.parse(
  execFileSync(binary, ["system", "capabilities", "--json"], {
    encoding: "utf8",
  }),
);
const domainsEnvelope = JSON.parse(
  execFileSync(binary, ["system", "domains", "--json"], { encoding: "utf8" }),
);
const eventSchemaEnvelope = JSON.parse(
  execFileSync(binary, ["system", "events-schema", "--json"], { encoding: "utf8" }),
);
if (domainsEnvelope.error || !Array.isArray(domainsEnvelope.data)) {
  throw new Error(domainsEnvelope.error?.message ?? "Invalid domain registry");
}
if (
  capabilitiesEnvelope.error ||
  !capabilitiesEnvelope.data ||
  !Array.isArray(capabilitiesEnvelope.data.job_kinds)
) {
  throw new Error(
    capabilitiesEnvelope.error?.message ?? "Invalid job schema registry",
  );
}
if (eventSchemaEnvelope.error || !eventSchemaEnvelope.data ||
    typeof eventSchemaEnvelope.data !== "object" || Array.isArray(eventSchemaEnvelope.data)) {
  throw new Error(eventSchemaEnvelope.error?.message ?? "Invalid event stream schema");
}
const pascal = (value) =>
  value
    .split(/[^a-zA-Z0-9]+/)
    .filter(Boolean)
    .map((part) => part[0].toUpperCase() + part.slice(1))
    .join("");
const isSchema = (value) =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const jobNames = new Set();
const jobNamespaces = new Set();
for (const job of capabilitiesEnvelope.data.job_kinds) {
  if (!job || typeof job !== "object" || typeof job.name !== "string" || !job.name) {
    throw new Error("Job schema registry contains an entry without a name");
  }
  if (jobNames.has(job.name)) {
    throw new Error(`Duplicate job schema name: ${job.name}`);
  }
  jobNames.add(job.name);
  const namespace = `Job${pascal(job.name)}`;
  if (jobNamespaces.has(namespace)) {
    throw new Error(`Job schema names collide as TypeScript namespace: ${namespace}`);
  }
  jobNamespaces.add(namespace);
  if (!isSchema(job.input_schema) || !isSchema(job.result_schema)) {
    throw new Error(`Job schema ${job.name} is missing input_schema or result_schema`);
  }
}
let output =
  "// Generated from the Rust automation registry. Do not edit.\n\n";
const toolNames = new Set();
for (const tool of envelope.data) {
  if (!/^opencut_[a-z0-9_]+$/.test(tool.name) || toolNames.has(tool.name)) throw new Error(`Invalid or duplicate tool: ${tool.name}`);
  toolNames.add(tool.name);
  for (const side of ["input_schema", "output_schema"]) {
    const namespace = `${pascal(tool.name)}${side === "input_schema" ? "Input" : "Output"}`;
    const source = await compile(tool[side], "Schema", {
      bannerComment: "",
      style: { singleQuote: true },
      unknownAny: false,
    });
    output += `export namespace ${namespace} {\n${source.replace(/^export /gm, "  export ")}\n}\n\n`;
  }
}
for (const job of capabilitiesEnvelope.data.job_kinds) {
  for (const side of ["input_schema", "result_schema"]) {
    const namespace = `Job${pascal(job.name)}${side === "input_schema" ? "Input" : "Result"}`;
    const source = await compile(job[side], "Schema", {
      bannerComment: "",
      style: { singleQuote: true },
      unknownAny: false,
    });
    output += `export namespace ${namespace} {\n${source.replace(/^export /gm, "  export ")}\n}\n\n`;
  }
}

output += "export interface ToolMap {\n";
for (const tool of envelope.data) {
  const title = tool.input_schema.title;
  if (!/^[A-Z][A-Za-z0-9]*$/.test(title) || tool.output_schema.title !== "Response") {
    throw new Error(`Unsupported root type in ${tool.name}`);
  }
  output += `  ${tool.name}: { input: ${pascal(tool.name)}Input.${title}; output: NonNullable<${pascal(tool.name)}Output.Response["data"]> };\n`;
}
output += "}\n";
const mappedTools = new Set();
output += "export interface DomainMap {\n";
for (const domain of domainsEnvelope.data) {
  if (!/^[a-z][a-z0-9_]*$/.test(domain.domain) || !Array.isArray(domain.actions)) {
    throw new Error("Invalid domain descriptor");
  }
  output += `  ${JSON.stringify(domain.domain)}: {\n`;
  const actions = new Set();
  for (const entry of domain.actions) {
    if (typeof entry.action !== "string" || !/^[a-z][a-z0-9.]*$/.test(entry.action)
        || actions.has(entry.action) || !toolNames.has(entry.operation)
        || mappedTools.has(entry.operation)) {
      throw new Error(`Invalid or duplicate domain action in ${domain.domain}`);
    }
    actions.add(entry.action);
    mappedTools.add(entry.operation);
    output += `    ${JSON.stringify(entry.action)}: ToolMap[${JSON.stringify(entry.operation)}];\n`;
  }
  output += "  };\n";
}
output += "}\n";
if (mappedTools.size !== toolNames.size) throw new Error("Domain registry does not cover every operation");
output += "export const domainNames = " + JSON.stringify(domainsEnvelope.data.map(d => d.domain), null, 2) + " as const;\n";
output += "export const domainActions = " + JSON.stringify(Object.fromEntries(domainsEnvelope.data.map(d => [d.domain, d.actions.map(a => a.action)])), null, 2) + " as const;\n";
output += "export interface JobResultMap {\n";
for (const job of capabilitiesEnvelope.data.job_kinds) {
  const title = job.result_schema.title;
  if (!/^[A-Z][A-Za-z0-9]*$/.test(title)) throw new Error(`Unsupported result type in ${job.name}`);
  output += `  "${job.name}": Job${pascal(job.name)}Result.${title};\n`;
}
output += "}\n";
output += "export const toolNames = " + JSON.stringify([...toolNames], null, 2) + " as const;\n";

// JSON Schema local refs must be rebased when embedded into an OpenAPI document.
const schemas = {};
function component(schema, name) {
  function rebase(value) {
    if (Array.isArray(value)) return value.map(rebase);
    if (value === null || typeof value !== "object") return value;
    return Object.fromEntries(Object.entries(value).map(([key, val]) => {
      if (key === "$ref") {
        if (typeof val !== "string" || !val.startsWith("#")) throw new Error("External schema refs are unsupported");
        return [key, `#/components/schemas/${name}${val.slice(1)}`];
      }
      return [key, rebase(val)];
    }));
  }
  schemas[name] = rebase(schema);
  return { $ref: `#/components/schemas/${name}` };
}
const paths = {};
for (const tool of envelope.data) {
  const input = component(tool.input_schema, `${pascal(tool.name)}Input`);
  const output = component(tool.output_schema, `${pascal(tool.name)}Output`);
  const response = { description: "OpenCut response envelope; inspect error.code on failure.", content: { "application/json": { schema: output } } };
  paths[`/v1/tools/${tool.name}`] = { post: {
    operationId: tool.name,
    description: tool.description,
    "x-opencut-read-only": tool.read_only,
    requestBody: { required: true, content: { "application/json": { schema: input } } },
    responses: { "200": response, default: response },
  } };
}
for (const domain of domainsEnvelope.data) {
  const variants = domain.actions.map((action) => ({
    type: "object",
    properties: {
      action: { const: action.action },
      input: { $ref: `#/components/schemas/${pascal(action.operation)}Input` },
    },
    required: ["action", "input"],
    additionalProperties: false,
  }));
  const resultVariants = domain.actions.map((action) => ({
    $ref: `#/components/schemas/${pascal(action.operation)}Output`,
  }));
  paths[`/v1/tools/opencut_${domain.domain}`] = { post: {
    operationId: `opencut_${domain.domain}`,
    description: `${domain.description} The action selects the exact legacy operation and its input schema.`,
    "x-opencut-action-operations": Object.fromEntries(domain.actions.map(a => [a.action, a.operation])),
    requestBody: { required: true, content: { "application/json": { schema: { oneOf: variants } } } },
    responses: {
      "200": { description: "Response envelope for the selected action.", content: { "application/json": { schema: { anyOf: resultVariants } } } },
      default: { description: "Structured OpenCut error response.", content: { "application/json": { schema: { type: "object" } } } },
    },
  } };
}
schemas.DomainActionDescriptor = {
  type: "object", required: ["action", "operation", "description", "read_only"],
  properties: {
    action: { type: "string" }, operation: { type: "string" },
    description: { type: "string" }, read_only: { type: "boolean" },
    input_schema: { type: "object" }, output_schema: { type: "object" },
  }, additionalProperties: false,
};
schemas.DomainDescriptor = {
  type: "object", required: ["domain", "description", "actions"],
  properties: {
    domain: { type: "string" }, description: { type: "string" },
    actions: { type: "array", items: { $ref: "#/components/schemas/DomainActionDescriptor" } },
  }, additionalProperties: false,
};
schemas.DomainCatalogEnvelope = {
  type: "object", required: ["api_version", "request_id", "data", "error", "warnings"],
  properties: {
    api_version: { const: "1" }, request_id: { type: "string" },
    data: { anyOf: [{ type: "array", items: { $ref: "#/components/schemas/DomainDescriptor" } }, { type: "null" }] },
    error: { anyOf: [{ type: "object" }, { type: "null" }] },
    warnings: { type: "array", items: { type: "string" } },
  }, additionalProperties: false,
};
const domainResponse = {
  description: "Authenticated domain catalog or exact per-action schema envelope.",
  content: { "application/json": { schema: { $ref: "#/components/schemas/DomainCatalogEnvelope" } } },
};
paths["/v1/domains"] = { get: {
  operationId: "opencut_domains_list", description: "Discover domains and action summaries without expanding per-action schemas.",
  responses: { "200": domainResponse, default: domainResponse },
} };
paths["/v1/domains/{domain}"] = { get: {
  operationId: "opencut_domain_inspect", description: "Get one domain with exact input/output schemas for every action.",
  parameters: [{ name: "domain", in: "path", required: true, schema: { type: "string", enum: domainsEnvelope.data.map(d => d.domain) } }],
  responses: { "200": domainResponse, default: domainResponse },
} };
schemas.OpenCutErrorEnvelope = {
  type: "object", required: ["api_version", "request_id", "data", "error", "warnings"],
  properties: {
    api_version: { const: "1" }, request_id: { type: "string" }, data: { type: "null" },
    error: { type: "object", required: ["code", "message", "details"],
      properties: { code: { type: "string" }, message: { type: "string" }, details: true },
      additionalProperties: false },
    warnings: { type: "array", items: { type: "string" } },
  }, additionalProperties: false,
};
const transportError = {
  description: "Structured OpenCut error envelope; inspect error.code before retrying.",
  content: { "application/json": { schema: { $ref: "#/components/schemas/OpenCutErrorEnvelope" } } },
};
const uploads = capabilitiesEnvelope.data.uploads;
if (uploads) {
  if (uploads.chunk_method !== "PUT" || uploads.chunk_path_template !== "/v1/uploads/{upload_id}/chunks/{offset}"
      || !Array.isArray(uploads.chunk_query_fields)
      || uploads.chunk_query_fields.join(",") !== "size_bytes,content_hash") {
    throw new Error("Unsupported upload byte transport in host capabilities");
  }
  paths[uploads.chunk_path_template] = { put: {
    operationId: "opencut_upload_chunk",
    description: `Send sequential raw bytes with a complete SHA-256 and canonical decimal offset/size. Maximum ${uploads.max_chunk_bytes} bytes per chunk, ${uploads.max_upload_bytes} bytes per upload, ${uploads.max_inflight_chunks} concurrent requests and ${uploads.chunk_timeout_seconds} seconds receiving. A lost response has an unknown outcome: inspect the confirmed next_offset before retrying identical bytes. Project read, edit and run_jobs permissions are required.`,
    parameters: [
      { name: "upload_id", in: "path", required: true, schema: { type: "string", pattern: "^upload_[a-f0-9]{32}$" } },
      { name: "offset", in: "path", required: true, schema: { type: "string", pattern: "^(0|[1-9][0-9]*)$" } },
      { name: "size_bytes", in: "query", required: true, schema: { type: "string", pattern: "^[1-9][0-9]*$" } },
      { name: "content_hash", in: "query", required: true, schema: { type: "string", pattern: "^[a-f0-9]{64}$" } },
    ],
    requestBody: { required: true, content: { "application/octet-stream": { schema: { type: "string", format: "binary" } } } },
    responses: {
      "200": { description: "Confirmed upload state and next_offset.", content: { "application/json": { schema: { $ref: "#/components/schemas/OpencutUploadInspectOutput" } } } },
      default: transportError,
    },
  } };
}
paths["/v1/artifacts/{artifact}"] = { get: {
  operationId: "opencut_artifact_download",
  description: "Download the bytes of a published job artifact identified by job ID and SHA-256. Authorization and integrity are checked against the committed job; a historical successful job does not guarantee that its file remains available. Content-Type identifies the actual media format.",
  parameters: [{ name: "artifact", in: "path", required: true,
    schema: { type: "string", pattern: "^artifact:job_[a-f0-9]{32}:[a-f0-9]{64}$" } }],
  responses: {
    "200": { description: "Verified artifact bytes, with actual Content-Type and Content-Length headers.",
      content: { "application/octet-stream": { schema: { type: "string", format: "binary" } } } },
    default: transportError,
  },
} };
paths["/v1/changes/stream"] = { post: {
  operationId: "opencut_changes_stream",
  description: "Observe public project or job changes with an opaque cursor. SSE data contains one opencut.events.v1 record; the SSE id is its resume cursor. Last-Event-ID overrides the body cursor. Reconnect using the last fully processed id. Closing observation does not cancel durable tasks.",
  "x-opencut-read-only": true,
  parameters: [{ name: "Last-Event-ID", in: "header", required: false, schema: { type: "string" } }],
  requestBody: { required: true, content: { "application/json": { schema: { $ref: "#/components/schemas/OpencutEventChangesInput" } } } },
  responses: {
    "200": { description: "SSE ready/change/error records; each data field is JSON matching x-opencut-data-schema.",
      content: { "text/event-stream": { schema: { type: "string" },
        "x-opencut-data-schema": component(eventSchemaEnvelope.data, "OpenCutEventStreamRecord") } } },
    default: transportError,
  },
} };
const openapi = {
  openapi: "3.1.0",
  jsonSchemaDialect: "https://json-schema.org/draft/2020-12/schema",
  info: { title: "OpenCut automation tool API", version: "1.0.0", description: "Domain and legacy automation endpoints plus raw project-upload chunks, published artifact bytes and public event SSE. Legacy v1 job paths are host-local and are not remote URLs." },
  security: [{ bearerAuth: [] }], paths,
  components: { securitySchemes: { bearerAuth: { type: "http", scheme: "bearer" } }, schemas },
};
const files = new Map([
  [target, output],
  [resolve(root, "packages/contract-fixtures/tools.generated.json"), JSON.stringify(envelope.data, null, 2) + "\n"],
  [resolve(root, "packages/contract-fixtures/openapi.generated.json"), JSON.stringify(openapi, null, 2) + "\n"],
  [resolve(root, "packages/contract-fixtures/event-stream.generated.json"), JSON.stringify(eventSchemaEnvelope.data, null, 2) + "\n"],
  [resolve(root, "apps/web/src/lib/editor-contracts.generated.ts"), '// Compatibility re-export; authoritative types live in the standalone SDK.\nexport * from "../../../../packages/sdk-ts/src/contracts.generated";\n'],
]);
for (const [path, content] of files) {
  if (check) {
    if (readFileSync(path, "utf8") !== content) throw new Error(`Generated contracts are stale: ${path}. Run npm run contracts:generate in packages/sdk-ts.`);
  } else {
    writeFileSync(path, content);
  }
}
console.log(check ? "Generated contracts are current." : "Generated SDK types, tool and event fixtures, OpenAPI and Web compatibility export.");
}
