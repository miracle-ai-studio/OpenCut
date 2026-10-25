import { editorSnapshot } from "./project-inspection.mjs";
import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { randomUUID } from "node:crypto";

// Root owns service/browser lifecycle. Reuse B1 project; offlineOnly must run
// against a genuinely download-disabled host, never a mocked network policy.
export async function verifyFontBrowser(browser, endpoint, directory, check, options = {}) {
  const page = await browser.newPage({locale:"en-US",viewport:{width:1440,height:1100}});
  page.setDefaultTimeout(30000);
  const errors = [], imports = [];
  page.on("pageerror", error => errors.push(error.message));
  page.on("request", request => { if (request.url().endsWith("/api/tools/opencut_font_import")) imports.push(request.postDataJSON()); });
  const checkpointPath = path.join(directory,"f20-font-browser-checkpoint.json");
  const id = process.env.OPENCUT_B1_PROJECT_ID || JSON.parse(await fs.readFile(path.join(directory,"b1-text-project.json"),"utf8")).project_id;
  const envelope = (name,input) => page.evaluate(async ({name,input}) => (await fetch(`/api/tools/${name}`,{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify(input)})).json(),{name,input});
  const tool = async (name,input) => { const result = await envelope(name,input); expect(result.error,JSON.stringify(result.error)).toBeNull(); return result.data; };
  const inspect = () => editorSnapshot(tool("opencut_project_inspect",{project_id:id,revision:null}));
  const render = async state => {
    await page.getByLabel("Playhead (seconds)",{exact:true}).fill("0.5");
    await page.getByRole("button",{name:"Go to time",exact:true}).click();
    const response = page.waitForResponse(r => r.url().endsWith("/api/tools/opencut_render_submit"));
    await page.getByRole("button",{name:"Render scene frame",exact:true}).click();
    const submitted = await response, result = await submitted.json();
    expect(result.error).toBeNull();
    expect(submitted.request().postDataJSON()).toMatchObject({kind:"frame",request:{revision:state.revision}});
    let job;
    await expect.poll(async () => { job = await tool("opencut_job_view",{job_id:result.data.job_id}); return ["succeeded","failed","cancelled"].includes(job.state) ? job.state : "pending"; },{timeout:120000,intervals:[250,500,1000]}).not.toBe("pending");
    expect(job.state, JSON.stringify({job_id:job.job_id,failure:job.failure})).toBe("succeeded");
    await expect(page.getByRole("img",{name:"Rendered scene frame",exact:true})).toBeVisible();
    return job.result.data;
  };
  try {
    await page.goto(`${endpoint}/editor`);
    await expect(page.getByRole("button",{name:"New project",exact:true})).toBeEnabled();
    await page.goto(`${endpoint}/editor/${encodeURIComponent(id)}`);
    const guide = page.getByRole("dialog",{name:"Welcome to OpenCut",exact:true});
    if (await guide.isVisible()) await guide.getByRole("button",{name:"Close",exact:true}).click();
    await expect(page.getByRole("main",{name:"OpenCut editor",exact:true})).toBeVisible();
    const catalog = await tool("opencut_font_catalog",{});
    if (options.offlineOnly) {
      expect(catalog.download_available).toBe(false);
      const saved = JSON.parse(await fs.readFile(checkpointPath,"utf8"));
      expect(saved.project_id).toBe(id);
      const current = await inspect();
      expect(current.revision).toBe(saved.render_revision);
      expect(current.project.fonts.some(f => f.id === saved.font_id && f.content.content_hash === saved.font_hash)).toBe(true);
      const title = current.project.sequences.flatMap(s => s.tracks.flatMap(t => t.elements)).find(e => e.id === saved.element_id);
      expect(title.content.text.content).toBe(saved.required_text);
      expect(title.content.text.font.font_id).toBe(saved.font_id);
      const result = await render(current);
      expect(result.artifact.content_hash).toBe(saved.png_hash);
      expect(result.font_lock.some(f => f.font_id === saved.font_id && f.content_hash === saved.font_hash)).toBe(true);
      expect(imports).toHaveLength(0);
      expect(errors).toEqual([]);
      await tool("opencut_project_restore",{project_id:id,expected_revision:current.revision,target_revision:saved.restore_revision,idempotency_key:randomUUID(),asset_versions:{}});
      check("F20 reopened project renders imported face with host font download disabled",{project_id:id,font_id:saved.font_id,png_hash:saved.png_hash,download_available:false,pageerrors:errors.slice(-8)});
      return;
    }
    expect(catalog.download_available).toBe(true);
    const before = await inspect();
    const family = catalog.families.find(f => f.family === "Lato");
    const reuse = process.env.OPENCUT_F20_REUSE_IMPORTED_FACE;
    const face = reuse ? family.faces.find(f => `${f.weight}:${f.style}` === reuse && f.catalog_id) : [[700,"normal"],[400,"italic"],[700,"italic"]].map(([weight,style]) => family.faces.find(f => f.weight === weight && f.style === style && f.catalog_id)).find(f => f && !before.project.fonts.some(locked => locked.family === "Lato" && locked.weight === f.weight && locked.style === f.style));
    expect(face,"B1 project needs an unimported static Lato face").toBeTruthy();
    let after = before, source;
    const text = "OpenCut Lato static face: Bold Italic 0123456789!";
    if (!reuse) {
    await page.getByRole("button",{name:"Import font",exact:true}).click();
    const form = page.getByRole("form",{name:"Import font",exact:true});
    await form.getByLabel("Search fonts",{exact:true}).fill("Lato");
    await form.getByRole("combobox",{name:/^Font family(?:\s|$)/i}).selectOption("Lato");
    const unavailable = family.faces.find(f => !f.catalog_id);
    expect(unavailable).toBeTruthy();
    await form.getByRole("combobox",{name:/^Font style(?:\s|$)/i}).selectOption(`${unavailable.weight}:${unavailable.style}`);
    await expect(form.getByRole("button",{name:"Import font",exact:true})).toBeDisabled();
    await expect(form.getByRole("status").filter({hasText:/not supported|No fixed static/})).toBeVisible();
    await form.getByRole("combobox",{name:/^Font style(?:\s|$)/i}).selectOption(`${face.weight}:${face.style}`);
    {const selected=await tool("opencut_font_source",{catalog_id:face.catalog_id});expect(selected.details.kind).toBe("static");source=selected.details.source;}
    await expect(form.getByRole("link",{name:"Font source and license",exact:true})).toHaveAttribute("href",source.source_url);
    await form.getByText("License notice",{exact:true}).click();
    await expect(form.locator("pre")).toHaveText(source.license_notice);
    await expect(form.getByRole("img",{name:"Lato family specimen",exact:true})).toBeVisible();
    await expect.poll(() => page.evaluate(() => { const node = document.querySelector('[aria-label="Lato family specimen"]'); const url = getComputedStyle(node).maskImage.match(/url\(["']?(.*?)["']?\)/)?.[1]; return url ? new Promise(resolve => { const image = new Image(); image.onload = () => resolve(image.naturalWidth > 0); image.onerror = () => resolve(false); image.src = url; }) : false; })).toBe(true);
    await form.getByRole("textbox",{name:/^Text to check(?:\s|$)/}).fill(text);
    const submitted = page.waitForResponse(r => r.url().endsWith("/api/tools/opencut_font_import"));
    await form.getByRole("button",{name:"Import font",exact:true}).click();
    const response = await submitted, accepted = await response.json();
    expect(accepted.error,JSON.stringify(accepted.error)).toBeNull();
    expect(response.request().postDataJSON()).toMatchObject({project_id:id,expected_revision:before.revision,catalog_id:face.catalog_id,required_text:text});
    await expect(form).toBeHidden();
    after = await inspect();
    expect(imports).toHaveLength(1);
    expect(after.revision).not.toBe(before.revision);
    expect(after.project.fonts).toHaveLength(before.project.fonts.length+1);
    expect({...after.project,fonts:before.project.fonts}).toEqual(before.project);
    const rejected = await envelope("opencut_font_import",{project_id:id,expected_revision:after.revision,idempotency_key:randomUUID(),catalog_id:face.catalog_id,font_id:randomUUID(),required_text:String.fromCodePoint(0x10ffff)});
    expect(rejected.error).toBeTruthy();
    expect(JSON.stringify(rejected.error)).toMatch(/GLYPH|glyph/);
    expect(await inspect()).toEqual(after);
    } else {
      // Resume only after the same candidate's catalog/import/rejection assertions
      // passed and a later fixture-name assertion failed. Retain that report.
      {const selected=await tool("opencut_font_source",{catalog_id:face.catalog_id});expect(selected.details.kind).toBe("static");source=selected.details.source;}
      expect(imports).toHaveLength(0);
    }
    const font = after.project.fonts.find(f => f.content.content_hash === source.content_hash && f.weight === face.weight && f.style === face.style);
    expect(font).toBeTruthy();
    const sequence = after.project.sequences.find(s => s.id === after.project.default_sequence_id);
    const title = sequence.tracks.filter(t => t.enabled && !t.locked).flatMap(t => t.elements).find(e => e.content.kind === "text" && e.enabled && after.project.fonts.some(f => f.id === e.content.text.font.font_id && f.family === "Lato"));
    expect(title).toBeTruthy();
    const restoreRevision = process.env.OPENCUT_F20_RESTORE_REVISION ?? after.revision;
    const restore = await editorSnapshot(tool("opencut_project_inspect",{project_id:id,revision:restoreRevision}));
    expect(restore.project.fonts.some(f => f.id === font.id && f.content.content_hash === source.content_hash)).toBe(true);
    await page.getByRole("combobox",{name:/^Scene(?:\s|$)/}).selectOption(sequence.id);
    if (title.content.text.content !== text || title.content.text.font.font_id !== font.id) {
    await page.locator("button.editor-element-select").filter({hasText:title.name}).click();
    await page.getByRole("button",{name:"Edit text",exact:true}).click();
    const edit = page.getByRole("form",{name:"Edit text",exact:true});
    await edit.getByRole("textbox",{name:/^Text content/}).fill(text);
    await edit.getByRole("combobox",{name:/^Font family(?:\s|$)/}).selectOption(font.id);
    await edit.getByRole("button",{name:"Save text",exact:true}).click();
    await page.getByRole("button",{name:"Apply text changes",exact:true}).click();
    await expect(page.getByRole("region",{name:"Review text changes",exact:true})).toHaveCount(0);
    await expect(edit).toBeHidden();
    await expect.poll(async () => {
      const updated = await inspect();
      const currentTitle = updated.project.sequences.flatMap(s => s.tracks.flatMap(t => t.elements)).find(e => e.id === title.id);
      return [currentTitle.content.text.content,currentTitle.content.text.font.font_id];
    }).toEqual([text,font.id]);
    }
    const renderedState = await inspect(), result = await render(renderedState);
    expect(result.font_lock.some(f => f.font_id === font.id && f.content_hash === source.content_hash)).toBe(true);
    const evidence = {project_id:id,element_id:title.id,catalog_id:face.catalog_id,font_id:font.id,font_hash:source.content_hash,family:"Lato",weight:face.weight,style:face.style,required_text:text,restore_revision:restoreRevision,render_revision:renderedState.revision,png_hash:result.artifact.content_hash,preview:"fixed Classic family atlas; not selected face/text",import_validation:reuse ? "reuse successful same-candidate catalog/import/missing-glyph assertions; retained failed report ends at stale fixture name" : "this run",offline_verification:"pending separate offlineOnly run"};
    await fs.writeFile(checkpointPath,JSON.stringify(evidence,null,2)+"\n");
    await page.screenshot({path:path.join(directory,"f20-font-browser.png"),fullPage:false});
    check("F20 catalog/style/license, atomic static import and missing glyph revision fence",{...evidence,pageerrors:errors.slice(-8)});
    expect(errors).toEqual([]);
  } catch (error) {
    const ui = await page.locator('main').ariaSnapshot().catch(() => "unavailable");
    await fs.writeFile(path.join(directory,"f20-font-browser-failure.json"),JSON.stringify({project_id:id,error:String(error),pageerrors:errors.slice(-8),ui:ui.slice(0,18000)},null,2)+"\n");
    await page.screenshot({path:path.join(directory,"f20-font-browser-failure.png"),fullPage:false}).catch(() => {});
    throw error;
  } finally { await page.close(); }
}
