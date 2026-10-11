import { test } from "node:test";
import assert from "node:assert/strict";
import { violations } from "../scripts/check-boundaries.mjs";
const web="apps/web/src/lib/example.ts";
test("TS boundary accepts local aliases and declared packages",()=>{
 assert.deepEqual(violations(web,'import {x} from "@/editor/x"; export {y} from "./y"; import "react";',new Set(["react"])),[]);
});
test("TS boundary rejects static, re-export, require and import-type escapes",()=>{
 for(const code of ['import {x} from "../../../../crates/native-services/internal";', 'export * from "../../../../packages/sdk-ts/src/transport";', 'const a=require("../../../../apps/api/src/index");','type T=import("../../../../apps/api/src/index").T;']) assert.equal(violations(web,code).length,1);
});
test("TS boundary rejects dynamic lookup and native SDK dependencies",()=>{
 assert.equal(violations(web,'const a=import(target);').length,1);
 assert.equal(violations("packages/sdk-ts/src/a.ts",'import fs from "node:fs";').length,1);
});
test("TS boundary permits only the public SDK bridge target",()=>{
 const bridge="apps/web/src/lib/editor-sdk.ts";
 assert.deepEqual(violations(bridge,'export * from "../../../../packages/sdk-ts/src/index";'),[]);
 assert.equal(violations(bridge,'export * from "../../../../packages/sdk-ts/src/transport";').length,1);
 assert.equal(violations(web,'import "../editor/generate-contracts.mjs";').length,1);
});

test("TS boundary rejects alias traversal and excluded runtime imports",()=>{
 assert.equal(violations(web,'import "@/../../api/src/index";').length,1);
 assert.equal(violations(web,'import "./other.test";').length,1);
 assert.equal(violations(web,'import "./other.spec.ts";').length,1);
 assert.equal(violations(web,'import "../editor/generate-contracts";').length,1);
 assert.equal(violations(web,'import "node:fs";',new Set(["node:fs"])).length,1);
});
