import { compile } from "json-schema-to-typescript";
import { generateContracts } from "./generate-core.mjs";
await generateContracts(compile);
