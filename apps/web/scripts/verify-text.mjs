import { editorSnapshot } from "./project-inspection.mjs";
import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { createHash } from "node:crypto";

// One owned B1 slice. No service startup, fixture copying, product build or
// renderer imports. The external font bytes drive a separate Canvas oracle.
export async function verifyText(browser, endpoint, directory, check, options = {}) {
  const page = await browser.newPage({ locale: "en-US", viewport: { width: 1440, height: 1100 } });
  page.setDefaultTimeout(30000);
  const errors = [];
  page.on("pageerror", error => errors.push(error.message));
  const metadata = path.join(directory, "b1-text-project.json");
  const fontsDirectory = process.env.OPENCUT_B1_FONT_SOURCES ?? "/private/tmp/opencut-b1-font-sources-20261013";
  const sourceManifest = JSON.parse(await fs.readFile(path.join(fontsDirectory, "manifest.json"), "utf8"));
  const fontSources = new Map();
  for (const source of sourceManifest.files.filter(item => item.file.endsWith(".ttf"))) {
    const bytes = await fs.readFile(path.join(fontsDirectory, path.basename(source.file)));
    const hash = createHash("sha256").update(bytes).digest("hex");
    expect(hash).toBe(source.sha256);
    fontSources.set(hash, { ...source, bytes: bytes.toString("base64") });
  }
  const tool = async (name, input) => {
    const envelope = await page.evaluate(async ({name,input}) => (await fetch(`/api/tools/${name}`, {
      method: "POST", headers: {"Content-Type":"application/json"}, body: JSON.stringify(input),
    })).json(), {name,input});
    expect(envelope.error, JSON.stringify(envelope.error)).toBeNull();
    return envelope.data;
  };
  let id;
  try {
    await page.goto(`${endpoint}/editor`);
    // The normal app bootstrap establishes its HttpOnly session before tools.
    await expect(page.getByRole("button", {name:"New project",exact:true})).toBeEnabled();
    if (process.env.OPENCUT_B1_PROJECT_ID) id = process.env.OPENCUT_B1_PROJECT_ID;
    else {
      try { id = JSON.parse(await fs.readFile(metadata,"utf8")).project_id; }
      catch (error) { if (error.code !== "ENOENT") throw error; }
    }
    if (id) await page.goto(`${endpoint}/editor/${encodeURIComponent(id)}`);
    else {
      await page.getByRole("button",{name:"New project",exact:true}).click();
      const dialog = page.getByRole("dialog",{name:"New project",exact:true});
      await dialog.getByLabel("Project name",{exact:true}).fill("B1 real text acceptance");
      await dialog.getByRole("button",{name:"Create project",exact:true}).click();
      await expect(page).toHaveURL(/\/editor\/.+/);
      id = decodeURIComponent(new URL(page.url()).pathname.split("/").at(-1));
      await fs.writeFile(metadata,JSON.stringify({project_id:id})+"\n");
    }
    const guide = page.getByRole("dialog",{name:"Welcome to OpenCut",exact:true});
    if (await guide.isVisible()) await guide.getByRole("button",{name:"Close",exact:true}).click();
    await expect(page.getByRole("main",{name:"OpenCut editor",exact:true})).toBeVisible();
    const inspect = () => editorSnapshot(tool("opencut_project_inspect",{project_id:id,revision:null}));
    const recoverOriginalFont = async () => {
      const observed = await tool("opencut_edit_recovery_inspect",{project_id:id,recovery_id:null});
      const original = observed.record;
      const panel = page.getByRole("region",{name:"Recover original edit",exact:true});
      if (original) {
        expect(original.project_id).toBe(id);
        expect(original.context.intent.kind, "Resolve any other original edit before this font journey").toBe("font_import");
        await expect(panel).toBeVisible();
        const refresh = panel.getByRole("button",{name:"Refresh recovery",exact:true});
        await expect(refresh).toBeEnabled();
        const refreshed = page.waitForResponse(response => response.url().endsWith("/api/tools/opencut_edit_recovery_inspect"),{timeout:45000});
        await refresh.click();
        const refreshedEnvelope = await (await refreshed).json();
        expect(refreshedEnvelope.error,JSON.stringify(refreshedEnvelope.error)).toBeNull();
        expect(refreshedEnvelope.data.record?.recovery_id).toBe(original.recovery_id);
        expect(refreshedEnvelope.data.record.context).toEqual(original.context);
        if (!refreshedEnvelope.data.record.receipt) {
          const retry = panel.getByRole("button",{name:"Retry original edit",exact:true});
          await expect(retry).toBeEnabled();
          const imported = page.waitForResponse(response => response.url().endsWith("/api/tools/opencut_font_import"),{timeout:45000});
          await retry.click();
          const response = await imported;
          // Recovery must replay its durable intent, never generate a new key.
          expect(response.request().postDataJSON()).toEqual(original.context.intent.request);
          const envelope = await response.json();
          expect(envelope.error,JSON.stringify(envelope.error)).toBeNull();
        }
        await expect(panel.getByRole("status")).toHaveText("Original edit saved.");
        // Retry performs its own ack; wait for that UI action to settle before
        // choosing release versus Done, avoiding a second concurrent ack.
        await expect(refresh).toBeEnabled();
        const release = panel.getByRole("button",{name:"Release resolved edit",exact:true});
        const done = panel.getByRole("button",{name:"Done",exact:true});
        await expect.poll(async () => await release.isVisible() ? "release" : await done.isVisible() ? "done" : "pending").not.toBe("pending");
        if (await release.isVisible()) {
          await expect(release).toBeEnabled();
          const acknowledged = page.waitForResponse(response => response.url().endsWith("/api/tools/opencut_edit_recovery_ack"),{timeout:45000});
          await release.click();
          const envelope = await (await acknowledged).json();
          expect(envelope.error,JSON.stringify(envelope.error)).toBeNull();
          expect(envelope.data).toMatchObject({recovery_id:original.recovery_id,resolution:"saved",receipt:{kind:"font_import"}});
        }
        await expect(done).toBeVisible();
        await done.click();
        await expect(panel).toBeHidden();
        check("B1 durable font recovery resolves its original intent and releases only a saved receipt",{project_id:id,recovery_id:original.recovery_id,original_idempotency_key:original.context.intent.request.idempotency_key,replayed_in_browser:!refreshedEnvelope.data.record.receipt});
      } else if (await panel.isVisible()) {
        const done = panel.getByRole("button",{name:"Done",exact:true});
        if (await done.isVisible()) {
          await done.click();
          await expect(panel).toBeHidden();
        }
      }
    };
    await recoverOriginalFont();
    let state = await inspect();
    const capability = await tool("opencut_render_capability",{});
    expect(capability.frame).toMatchObject({available:true});
    expect(capability.submit_tool).toBe("opencut_render_submit");
    if (options.layoutOnly) {
      const catalog = await tool("opencut_font_catalog",{});
      expect(catalog.download_available).toBe(false);
      const { verifyTextLayout } = await import("./verify-text-layout.mjs");
      await verifyTextLayout({page,tool,inspect,projectId:id,sequenceId:state.project.default_sequence_id,fontSources,directory,check});
      expect(errors).toEqual([]);
      check("B1 imported fonts reopen and render with public font downloads disabled",{project_id:id,download_available:false,pageerrors:errors});
      return;
    }
    for (const family of ["Lato","Ma Shan Zheng"]) {
      if (state.project.fonts.some(font => font.family === family && font.weight === 400 && font.style === "normal")) continue;
      await page.getByRole("button",{name:"Import font",exact:true}).click();
      const form = page.getByRole("form",{name:"Import font",exact:true});
      await expect(form).toBeVisible();
      await form.getByRole("combobox",{name:/^Font face(?:\s|$)/}).selectOption({label:`${family} · 400 · Regular`});
      await form.getByRole("textbox",{name:/^Text to check/}).fill(family === "Lato" ? "OpenCut" : "你好世界");
      const imported = page.waitForResponse(response => response.url().endsWith("/api/tools/opencut_font_import"),{timeout:45000});
      await form.getByRole("button",{name:"Import font",exact:true}).click();
      const importEnvelope = await (await imported).json();
      expect(importEnvelope.error, JSON.stringify(importEnvelope.error)).toBeNull();
      await expect(form).toBeHidden();
      state = await inspect();
    }
    const fonts = state.project.fonts;
    for (const font of fonts) if (["Lato","Ma Shan Zheng"].includes(font.family)) expect(fontSources.has(font.content.content_hash)).toBe(true);
    const titleName = "B1 independent title";
    const sequenceId = state.project.default_sequence_id;
    await page.getByRole("combobox",{name:/^Scene(?:\s|$)/}).selectOption(sequenceId);
    const findTitle = current => current.project.sequences.find(scene => scene.id === sequenceId).tracks.flatMap(track => track.elements).find(element => element.name === titleName);
    const save = async (mode, content, family) => {
      if (mode === "create") await page.getByRole("button",{name:"Add text at playhead",exact:true}).click();
      else {
        // Selection is through the actual timeline, not a hidden controller.
        await page.locator("button.editor-element-select").filter({hasText:titleName}).click();
        await page.getByRole("button",{name:"Edit text",exact:true}).click();
      }
      const form = page.getByRole("form",{name:mode === "create" ? "New text" : "Edit text",exact:true});
      await form.getByLabel("Text name",{exact:true}).fill(titleName);
      await form.getByRole("textbox",{name:/^Text content/}).fill(content);
      await form.getByRole("combobox",{name:/^Font family(?:\s|$)/}).selectOption(fonts.find(font => font.family === family && font.weight === 400 && font.style === "normal").id);
      await form.getByLabel("Font size (Classic)",{exact:true}).fill("9");
      await form.getByLabel("Duration (seconds)",{exact:true}).fill("2");
      await form.getByLabel("Start (seconds)",{exact:true}).fill("0");
      await form.getByRole("button",{name:"Save text",exact:true}).click();
      await page.getByRole("button",{name:"Apply text changes",exact:true}).click();
      await expect(form).toBeHidden();
      await expect.poll(async () => findTitle(await inspect())?.content.text.content).toBe(content);
      return inspect();
    };
    const existingTitle = findTitle(state);
    const latinFont = fonts.find(font => font.family === "Lato" && font.weight === 400 && font.style === "normal");
    const seconds = time => Number(time.ticks) / time.timebase;
    const reusableLatin = existingTitle?.content.kind === "text" &&
      existingTitle.content.text.content === "OpenCut" &&
      existingTitle.content.text.font.kind === "locked" && existingTitle.content.text.font.font_id === latinFont.id &&
      existingTitle.content.text.font_size.unit === "classic_height90" && existingTitle.content.text.font_size.value === 9 &&
      seconds(existingTitle.timeline_range.start) === 0 && seconds(existingTitle.timeline_range.duration) === 2;
    // A prior render failure may leave the already-saved Latin baseline. Reuse
    // that revision rather than asking the product to save an unchanged draft.
    if (!reusableLatin) state = await save(existingTitle ? "edit" : "create","OpenCut","Lato");
    const render = async (fixed, content, family) => {
      await page.getByLabel("Playhead (seconds)",{exact:true}).fill("0.5");
      await page.getByRole("button",{name:"Go to time",exact:true}).click();
      const submit = page.waitForResponse(response => response.url().endsWith("/api/tools/opencut_render_submit"));
      await page.getByRole("button",{name:"Render scene frame",exact:true}).click();
      const response = await submit, envelope = await response.json(), request = response.request().postDataJSON();
      expect(envelope.error).toBeNull();
      expect(request).toMatchObject({kind:"frame",request:{project_id:id,revision:fixed.revision,sequence_id:sequenceId}});
      let job;
      await expect.poll(async () => { job = await tool("opencut_job_view",{job_id:envelope.data.job_id}); return ["succeeded","failed","cancelled"].includes(job.state) ? job.state : "pending"; }, {timeout:120000,intervals:[100,250,500]}).not.toBe("pending");
      expect(job.state,JSON.stringify(job.failure)).toBe("succeeded");
      expect(job.result).toMatchObject({kind:"render.frame"});
      const result = job.result.data;
      expect(result).toMatchObject({identity:{project_id:id,revision:fixed.revision,sequence_id:sequenceId,time:request.request.time,profile:"classic-v31-gpu-scene-frame-v1"}});
      const font = fonts.find(font => font.family === family && font.weight === 400 && font.style === "normal");
      expect(result.font_lock).toEqual([{font_id:font.id,content_hash:font.content.content_hash,size:font.content.size}]);
      expect(result.texts).toHaveLength(1);
      const image = page.getByRole("img",{name:"Rendered scene frame",exact:true});
      await expect(image).toBeVisible();
      const canvasSize = fixed.project.settings.settings.canvas_size;
      const oracle = await image.evaluate(async (image,{bytes,content,width,height}) => {
        await image.decode();
        const face = new FontFace("B1IndependentOracle",Uint8Array.from(atob(bytes),c => c.charCodeAt(0)));
        await face.load(); document.fonts.add(face);
        try {
          const expected = new OffscreenCanvas(width,height), ctx = expected.getContext("2d");
          ctx.fillStyle = "black"; ctx.fillRect(0,0,width,height);
          ctx.font = `${height / 10}px B1IndependentOracle`; ctx.textAlign = "center"; ctx.textBaseline = "middle";
          ctx.fillStyle = "white"; ctx.fillText(content,width/2,height/2);
          const measuredWidth = ctx.measureText(content).width;
          const actual = new OffscreenCanvas(width,height), actualCtx = actual.getContext("2d");
          actualCtx.drawImage(image,0,0);
          const a = actualCtx.getImageData(0,0,width,height).data, e = ctx.getImageData(0,0,width,height).data;
          let union=0,intersection=0,maxDifference=0,backgroundError=0;
          const bbox = pixels => { let left=width,top=height,right=-1,bottom=-1,count=0; for(let y=0;y<height;y++) for(let x=0;x<width;x++) if(pixels[(y*width+x)*4]>32){count++;left=Math.min(left,x);right=Math.max(right,x);top=Math.min(top,y);bottom=Math.max(bottom,y);} return {left,top,right,bottom,count}; };
          for(let i=0;i<a.length;i+=4){const onA=a[i]>32,onE=e[i]>32;union+=Number(onA||onE);intersection+=Number(onA&&onE);maxDifference=Math.max(maxDifference,Math.abs(a[i]-e[i]));if(e[i]===0)backgroundError=Math.max(backgroundError,a[i],a[i+1],a[i+2],255-a[i+3]);}
          return {width:image.naturalWidth,height:image.naturalHeight,actual:bbox(a),expected:bbox(e),iou:intersection/union,maxDifference,backgroundError,measuredWidth,margin:[...actualCtx.getImageData(0,0,1,1).data]};
        } finally {document.fonts.delete(face);}
      },{bytes:fontSources.get(font.content.content_hash).bytes,content,width:canvasSize.width,height:canvasSize.height});
      expect(oracle.width).toBe(canvasSize.width); expect(oracle.height).toBe(canvasSize.height);
      expect(oracle.expected.count).toBeGreaterThan(100); expect(oracle.iou).toBeGreaterThan(.995);
      for(const axis of ["left","top","right","bottom"]) expect(Math.abs(oracle.actual[axis]-oracle.expected[axis])).toBeLessThanOrEqual(1);
      expect(oracle.margin).toEqual([0,0,0,255]);
      expect(oracle.backgroundError).toBeLessThanOrEqual(2);
      expect(result.texts[0].layout.lines[0].width).toBeCloseTo(oracle.measuredWidth,4);
      check(`B1 ${family} real PNG independent font geometry and glyph pixels`,{project_id:id,revision:fixed.revision,identity:result.identity,font_lock:result.font_lock,execution:result.execution,texts:result.texts,artifact:result.artifact,oracle});
      return result.artifact.content_hash;
    };
    const latinHash = await render(state,"OpenCut","Lato");
    state = await save("edit","你好世界","Ma Shan Zheng");
    const chineseHash = await render(state,"你好世界","Ma Shan Zheng");
    expect(chineseHash).not.toBe(latinHash);
    await page.getByRole("button",{name:"Undo",exact:true}).click();
    await page.getByRole("button",{name:"Apply undo",exact:true}).click();
    await expect.poll(async()=>findTitle(await inspect())?.content.text.content).toBe("OpenCut");
    state = await inspect(); expect(await render(state,"OpenCut","Lato")).toBe(latinHash);
    await page.getByRole("button",{name:"Redo",exact:true}).click();
    await page.getByRole("button",{name:"Apply redo",exact:true}).click();
    await expect.poll(async()=>findTitle(await inspect())?.content.text.content).toBe("你好世界");
    await page.reload();
    await expect(page.getByRole("main",{name:"OpenCut editor",exact:true})).toBeVisible();
    state = await inspect(); expect(findTitle(state).content.text.content).toBe("你好世界");
    expect(await render(state,"你好世界","Ma Shan Zheng")).toBe(chineseHash);
    expect(errors).toEqual([]);
    check("B1 real text create/edit, revision-owned undo/redo and reopen preserve independently verified Chinese/Latin PNGs",{project_id:id,revision:state.revision,latin_png_sha256:latinHash,chinese_png_sha256:chineseHash,pageerrors:errors});
  } catch (error) {
    // Bounded synthetic UI evidence survives page disposal. Do not retain
    // transport headers, credentials, font bytes or an unbounded page dump.
    const failure = { project_id: id ?? null, url: page.url(), error: String(error), pageerrors: errors.slice(-16) };
    try {
      failure.ui = await page.evaluate(() => ({
        forms: [...document.querySelectorAll("form")].slice(0,16).map(form => ({
          aria_label: form.getAttribute("aria-label"), text: form.textContent.slice(0,4096),
        })),
        labels: [...document.querySelectorAll("label")].slice(0,80).map(label => ({
          text: label.textContent.slice(0,1024), span: label.querySelector("span")?.textContent.slice(0,128),
          controls: [...label.querySelectorAll("input,textarea,select")].slice(0,4).map(control => ({
            tag: control.tagName, type: control.getAttribute("type"), disabled: control.disabled,
            aria_label: control.getAttribute("aria-label"),
            options: control.tagName === "SELECT" ? [...control.options].slice(0,24).map(option => ({value:option.value,text:option.text,selected:option.selected})) : undefined,
          })),
        })),
        alerts: [...document.querySelectorAll('[role="alert"],[role="status"]')].slice(0,16).map(node => node.textContent.slice(0,1024)),
      }));
    } catch (capture) { failure.capture_error = String(capture); }
    await fs.writeFile(path.join(directory,"b1-text-failure.json"),JSON.stringify(failure,null,2)+"\n");
    await page.screenshot({path:path.join(directory,"b1-text-failure.png"),fullPage:false}).catch(() => {});
    throw error;
  } finally { await page.close(); }
}
