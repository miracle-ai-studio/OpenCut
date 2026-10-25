/** One retained import: real UI response-loss/reload/replay and optional history
 * cycle. No package/media fixture copy,capability qualification or new output. */
import { editorSnapshot } from "./project-inspection.mjs";
import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { OpenCutClient } from "../../../packages/sdk-ts/dist/index.js";
const read=async file=>JSON.parse(await fs.readFile(file,"utf8"));
const sha=bytes=>createHash("sha256").update(bytes).digest("hex");
async function tree(root){
  const entries=[];
  async function visit(folder){for(const e of await fs.readdir(folder,{withFileTypes:true})){const file=path.join(folder,e.name);if(e.isDirectory())await visit(file);else if(/\.(?:js|mjs|cjs|css|json)$/.test(e.name))entries.push([path.relative(root,file),sha(await fs.readFile(file))]);}}
  await visit(root);entries.sort((a,b)=>a[0].localeCompare(b[0]));expect(entries.length).toBeGreaterThan(0);return sha(JSON.stringify(entries));
}
export async function verifyProjectPackage(browser,endpoint,directory,check){
  const sourceFile=process.env.OPENCUT_PACKAGE_CHECKPOINT;
  expect(sourceFile,"Use the completed retained verify-package-exchange.py checkpoint").toBeTruthy();
  const retained=await read(sourceFile),workspace=retained.scope.target;
  const inspected=retained.stages["export-and-checked-manifest"].result,
    receipt=retained.stages["atomic-import-and-original-replay"].result,request=retained.import_request;
  expect(retained.stages["independent-content-resources-and-replay"]).toBeTruthy();
  const service=await read(path.join(workspace,"service/state.json"));
  expect(await fs.realpath(service.database)).toBe(await fs.realpath(path.join(workspace,"workspace.sqlite")));
  expect(sha(await fs.readFile(service.executable))).toBe(retained.scope.candidate);
  const identity={native:retained.scope.candidate,web:await tree(service.web_root),sdk:await tree(new URL("../../../packages/sdk-ts/dist",import.meta.url).pathname),request,history_cycle:process.env.OPENCUT_PACKAGE_HISTORY_CYCLE==="1"};
  const checkpoint=path.join(directory,"package-ui-checkpoint.json");
  const state=await read(checkpoint).catch(e=>{if(e.code!=="ENOENT")throw e;return {identity,stages:{},requests:{}};});
  expect(state.identity).toEqual(identity);
  const save=async()=>{await fs.writeFile(checkpoint+".pending",JSON.stringify(state,null,2)+"\n");await fs.rename(checkpoint+".pending",checkpoint);};
  const stage=async(name,run)=>{if(state.stages[name])return;state.stages[name]=await run();await save();};
  const counts=()=>JSON.parse(execFileSync("python3",["-c",`import sqlite3,json,sys
c=sqlite3.connect('file:'+sys.argv[1]+'?mode=ro',uri=True)
print(json.dumps({t:c.execute('select count(*) from '+t).fetchone()[0] for t in ['projects','revisions','editor_transactions','jobs']}))`,path.join(workspace,"workspace.sqlite")],{encoding:"utf8"}));
  const client=new OpenCutClient({baseUrl:endpoint,token:(await fs.readFile(path.join(workspace,"service/api-token"),"utf8")).trim()});
  const project=request.target_project_id,inspect=()=>editorSnapshot(client.call("opencut_project_inspect",{project_id:project,revision:null}));
  state.initial??={counts:counts(),snapshot:await inspect()};await save();
  const context=await browser.newContext({locale:"en-US",viewport:{width:1440,height:1000}}),errors=[];
  let release;
  try{
    await context.route("**/api/tools/*_capability",r=>r.abort("failed"));
    // Covers now use the shared render entry directly. They are outside this
    // package/history acceptance and must not enqueue work on library reload.
    await context.route("**/api/tools/opencut_render_submit",r=>
      r.request().postDataJSON().kind==="frame" ? r.abort("failed") : r.continue());
    const page=await context.newPage();page.on("pageerror",e=>errors.push(e.message));
    const library=async()=>{await page.goto(endpoint+"/projects");await expect(page.getByRole("button",{name:"New project",exact:true})).toBeEnabled();};
    const dialog=async()=>{await page.getByRole("button",{name:"Restore project package",exact:true}).click();return page.getByRole("dialog",{name:"Restore project package",exact:true});};
    await stage("original-response-loss-reload-and-replay",async()=>{
      await library();const access=await client.call("opencut_access_inspect",{});expect(access.owner).toBe(true);
      const connection=JSON.stringify([client.baseUrl,access.principal_id,access.credential_id??null,access.owner]);
      await page.evaluate(v=>sessionStorage.setItem("opencut.package",JSON.stringify(v)),{entries:[{identity:connection,value:{request,inspection:inspected}}]});
      let held=false,dropped=false;const gate=new Promise(resolve=>{release=resolve;}),observed=[];
      await context.route("**/api/tools/opencut_project_package_import",async route=>{
        const body=route.request().postDataJSON();expect(body).toEqual(request);observed.push(body);
        const response=await route.fetch();expect((await response.json()).data).toEqual(receipt);
        if(!dropped){dropped=true;held=true;await gate;await route.abort("failed").catch(()=>{});}else await route.fulfill({response});
      });
      await (await dialog()).getByRole("button",{name:"Retry restore",exact:true}).click();await expect.poll(()=>held).toBe(true);
      await page.reload();release();await expect(page.getByRole("button",{name:"New project",exact:true})).toBeEnabled();
      expect(await page.evaluate(()=>JSON.parse(sessionStorage.getItem("opencut.package")).entries.length)).toBe(1);
      const modal=await dialog();await modal.getByRole("button",{name:"Retry restore",exact:true}).click();await expect(modal.getByText("Project restored",{exact:true})).toBeVisible();
      expect(observed).toHaveLength(2);expect(counts()).toEqual(state.initial.counts);
      expect(await page.evaluate(()=>JSON.parse(sessionStorage.getItem("opencut.package")).entries)).toEqual([]);
      await modal.getByRole("button",{name:"Open restored project",exact:true}).click();await expect(page.getByRole("region",{name:"Timeline",exact:true})).toBeVisible();
      await page.reload();await expect(page.getByRole("region",{name:"Timeline",exact:true})).toBeVisible();expect(await inspect()).toEqual(state.initial.snapshot);
      await context.unroute("**/api/tools/opencut_project_package_import");
      return {original_producer:"CLI",exact_replays:2,withheld_response_and_full_reload:true,recovery_removed_after_valid_receipt:true,counts:counts()};
    });
    if(identity.history_cycle)await stage("imported-history-cycle",async()=>{
      expect(inspected.scope).toBe("history");await page.goto(endpoint+"/editor/"+project);
      const history=page.locator(".editor-history");await expect(history).toBeVisible();
      const navigation=state.navigation??await client.call("opencut_project_history_navigation",{project_id:project,revision:null});state.navigation=navigation;await save();
      const first=navigation.redo?"redo":"undo",second=first==="redo"?"undo":"redo";
      expect(navigation[first]).not.toBeNull();
      for(const name of [first,second]){
        if(state[name+"_receipt"])continue;
        const tool="opencut_edit_"+name+"_apply";
        await context.route("**/api/tools/"+tool,async route=>{const body=route.request().postDataJSON();if(state.requests[name])expect(body).toEqual(state.requests[name]);else{state.requests[name]=body;await save();}await route.continue();});
        await history.getByRole("button",{name:name==="redo"?"Redo":"Undo",exact:true}).click();
        const action=history.getByRole("button",{name:name==="redo"?"Apply redo":"Apply undo",exact:true});await expect(action).toBeEnabled();
        const response=page.waitForResponse(r=>new URL(r.url()).pathname.endsWith("/"+tool));await action.click();const envelope=await(await response).json();expect(envelope.error).toBeNull();state[name+"_receipt"]=envelope.data;await save();
        expect(await client.call(tool,state.requests[name])).toEqual(envelope.data);await context.unroute("**/api/tools/"+tool);
      }
      await page.reload();await expect(page.getByRole("region",{name:"Timeline",exact:true})).toBeVisible();expect((await inspect()).project).toEqual(state.initial.snapshot.project);
      expect(counts()).toEqual({...state.initial.counts,revisions:state.initial.counts.revisions+2,editor_transactions:state.initial.counts.editor_transactions+2});
      return {causal_history_used_after_import:true,original_requests_replayed:true,complete_content_restored:true};
    });
    expect(errors).toEqual([]);expect(await tree(service.web_root)).toBe(identity.web);state.page_errors=errors;state.verifier_sha256=sha(await fs.readFile(import.meta.filename));
    state.scope=["Same already-restored project and original request;no fixture/media/output copies.","Native resource/graph proof is retained separately for the same binary.",...(!identity.history_cycle?["History mutation cycle was not requested."]:[])];await save();
    check("package import UI resumes the original reviewed request",true);
  }catch(e){state.failure={message:e.message,page_errors:errors};await save();throw e;}finally{
    release?.();await context.request.delete(endpoint+"/app/session",{headers:{Origin:endpoint,"X-OpenCut-App":"web"}}).catch(()=>{});await context.close();
  }
}
