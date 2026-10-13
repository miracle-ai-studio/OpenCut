import { expect } from "@playwright/test";
import fs from "node:fs/promises";
import path from "node:path";
import { randomUUID } from "node:crypto";

// Independent acceptance only: no renderer module or receipt drives the oracle.
// Shares the existing synthetic project, fonts, authenticated page and service.
export async function verifyV4TextLayout({page,tool,inspect,projectId,sequenceId,fontSources,directory,check}) {
  const original = await inspect();
  expect(original.project.settings.profile).toBe("classic_global");
  const scene = original.project.sequences.find(scene => scene.id === sequenceId);
  const track = scene.tracks.find(track => track.elements.some(element => element.name === "B1 independent title"));
  const title = track?.elements.find(element => element.name === "B1 independent title");
  expect(title?.content.kind).toBe("text");
  const metadata = path.join(directory,"b1-text-layout-state.json");
  await fs.writeFile(metadata,JSON.stringify({project_id:projectId,sequence_id:sequenceId,original_revision:original.revision,status:"running"})+"\n");
  let changed = false;
  try {
    for (const [family,content] of [["Lato","AV To\nOpenCut"],["Ma Shan Zheng","你好\n世界"]]) {
      const font = original.project.fonts.find(font => font.family === family && font.weight === 400 && font.style === "normal");
      expect(font).toBeTruthy();
      const source = fontSources.get(font.content.content_hash);
      expect(source).toBeTruthy();
      const current = await inspect();
      const settings = structuredClone(original.project.settings);
      settings.settings.canvas_size = {width:320,height:180};
      settings.settings.canvas_size_mode = "custom";
      settings.settings.last_custom_canvas_size = {width:320,height:180};
      settings.settings.background = {kind:"color",color:{encoding:"srgb",red:0,green:0,blue:0,alpha:1}};
      const element = structuredClone(title);
      element.enabled = true;
      element.channels = [];
      element.timeline_range = {start:{ticks:"0",timebase:1},duration:{ticks:"2",timebase:1}};
      Object.assign(element.content.text, {
        content,font:{kind:"locked",font_id:font.id},font_weight:"normal",font_style:"normal",
        align:"center",decoration:"underline",font_size:{unit:"classic_height90",value:15},
        letter_spacing_px:2,line_height_multiplier:1.2,
        color:{encoding:"srgb",red:1,green:1,blue:1,alpha:1},
        background:{enabled:true,color:{encoding:"srgb",red:1,green:0,blue:0,alpha:1},
          padding_x_at_size15:4,padding_y_at_size15:3,offset_x_px:5,offset_y_px:-7,corner_radius_percent:40},
      });
      Object.assign(element.visual, {opacity:1,blend_mode:"normal",effects:[],masks:[],
        transform:{position_x_px:0,position_y_px:0,scale_x:1.25,scale_y:.75,rotation_degrees:90}});
      // Disable other elements in this owned scene so the full-frame expected
      // pixels have one explicit source. Restore returns all original values.
      const operations = [{type:"settings_set",settings}];
      for (const ownedTrack of scene.tracks) for (const other of ownedTrack.elements) {
        const updated = other.id === title.id ? element : {...structuredClone(other),enabled:false};
        operations.push({type:"element_put",sequence_id:sequenceId,track_id:ownedTrack.id,element:updated});
      }
      await tool("opencut_edit_v4_apply",{project_id:projectId,expected_revision:current.revision,
        idempotency_key:randomUUID(),label:`B1 independent ${family} multiline layout`,operations});
      changed = true;
      const fixed = await inspect();
      await page.reload();
      await expect(page.getByRole("main",{name:"V4 editor",exact:true})).toBeVisible();
      await page.getByRole("combobox",{name:/^V4 scene(?:\s|$)/}).selectOption(sequenceId);
      await page.getByLabel("Playhead (seconds)",{exact:true}).fill("0");
      await page.getByRole("button",{name:"Go to time",exact:true}).click();
      const submitted = page.waitForResponse(response => response.url().endsWith("/v1/tools/opencut_job_v4_text_frame_png"));
      await page.getByRole("button",{name:"Render scene frame",exact:true}).click();
      const response = await submitted, envelope = await response.json();
      expect(envelope.error,JSON.stringify(envelope.error)).toBeNull();
      expect(response.request().postDataJSON()).toMatchObject({project_id:projectId,revision:fixed.revision,sequence_id:sequenceId});
      const frameTime = response.request().postDataJSON().time;
      expect(Number(frameTime.ticks)/frameTime.timebase).toBe(0);
      let job;
      await expect.poll(async()=>{job=await tool("opencut_job_view",{job_id:envelope.data.job_id});return ["succeeded","failed","cancelled"].includes(job.state)?job.state:"pending";},{timeout:120000,intervals:[100,250,500]}).not.toBe("pending");
      expect(job.state,JSON.stringify(job.failure)).toBe("succeeded");
      expect(job.extended_result).toMatchObject({kind:"render.project_text_frame.v4",schema_version:1});
      const result = job.extended_result.data;
      expect(result.identity).toMatchObject({project_id:projectId,revision:fixed.revision,sequence_id:sequenceId,width:320,height:180,profile:"classic-v31-gpu-normal-text-frame-v2"});
      expect(result.texts).toHaveLength(1);
      const image = page.getByRole("img",{name:"Rendered scene frame",exact:true});
      await expect(image).toBeVisible();
      const oracle = await image.evaluate(async(image,{bytes,content})=>{
        await image.decode();
        const face=new FontFace("B1LayoutOracle",Uint8Array.from(atob(bytes),char=>char.charCodeAt(0)));
        await face.load();document.fonts.add(face);
        try {
          const canvas=new OffscreenCanvas(320,180),ctx=canvas.getContext("2d");
          ctx.fillStyle="black";ctx.fillRect(0,0,320,180);
          // Height180 / Classic90 × size15 =30. Two lines at 1.2 ×30
          // are 36px apart; their middle baselines are -18 and +18.
          ctx.font="30px B1LayoutOracle";ctx.textAlign="center";ctx.textBaseline="middle";ctx.letterSpacing="2px";
          const lines=content.split("\n").map((text,index)=>{const m=ctx.measureText(text);return {text,width:m.width,ascent:m.actualBoundingBoxAscent,descent:m.actualBoundingBoxDescent,baselineY:index===0?-18:18};});
          const maxWidth=Math.max(...lines.map(line=>line.width));
          // Classic padding uses authored size15 /15 =1, independently
          // of output font30px. Padding stays4/3 and offsets stay5/-7.
          const rectangle={left:-maxWidth/2-4+5,top:-36-3-7,width:maxWidth+8,height:72+6};
          rectangle.radius=Math.min(rectangle.width,rectangle.height)*.2;
          // T·S·R: x'=160-1.25y, y'=90+.75x for the 90° rotation.
          const world=(x,y)=>({x:160-1.25*y,y:90+.75*x});
          const center=world(rectangle.left+rectangle.width/2,rectangle.top+rectangle.height/2);
          const corners=[world(rectangle.left,rectangle.top),world(rectangle.left+rectangle.width,rectangle.top),world(rectangle.left,rectangle.top+rectangle.height),world(rectangle.left+rectangle.width,rectangle.top+rectangle.height)];
          ctx.save();ctx.translate(160,90);ctx.scale(1.25,.75);ctx.rotate(Math.PI/2);
          ctx.fillStyle="red";ctx.beginPath();ctx.roundRect(rectangle.left,rectangle.top,rectangle.width,rectangle.height,rectangle.radius);ctx.fill();
          ctx.fillStyle="white";
          for(const line of lines){ctx.fillText(line.text,0,line.baselineY);ctx.fillRect(-line.width/2,line.baselineY+line.descent+2.1,line.width,2.1);}
          ctx.restore();
          const actual=new OffscreenCanvas(320,180),a=actual.getContext("2d");a.drawImage(image,0,0);
          const expected=ctx.getImageData(0,0,320,180).data,observed=a.getImageData(0,0,320,180).data;
          let union=0,intersection=0,sumError=0,blankError=0;
          const bounds=p=>{let left=320,top=180,right=-1,bottom=-1,count=0;for(let y=0;y<180;y++)for(let x=0;x<320;x++)if(p[(y*320+x)*4]>32){count++;left=Math.min(left,x);top=Math.min(top,y);right=Math.max(right,x);bottom=Math.max(bottom,y);}return {left,top,right,bottom,count};};
          for(let i=0;i<expected.length;i+=4){const onE=expected[i]>32,onA=observed[i]>32;union+=Number(onE||onA);intersection+=Number(onE&&onA);for(let c=0;c<4;c++)sumError+=Math.abs(expected[i+c]-observed[i+c]);if(expected[i]===0&&expected[i+1]===0&&expected[i+2]===0)blankError=Math.max(blankError,observed[i],observed[i+1],observed[i+2],255-observed[i+3]);}
          const pixels=[{x:0,y:0},{x:319,y:179},{x:Math.round(center.x),y:Math.round(center.y)}].map(point=>({...point,expected:[...ctx.getImageData(point.x,point.y,1,1).data],actual:[...a.getImageData(point.x,point.y,1,1).data]}));
          return {lines,rectangle,center,corners,expectedBounds:bounds(expected),actualBounds:bounds(observed),iou:intersection/union,meanChannelError:sumError/expected.length,blankError,pixels,width:image.naturalWidth,height:image.naturalHeight};
        }finally{document.fonts.delete(face);}
      },{bytes:source.bytes,content});
      expect(oracle.width).toBe(320);expect(oracle.height).toBe(180);
      expect(oracle.iou).toBeGreaterThan(.995);expect(oracle.meanChannelError).toBeLessThan(.1);expect(oracle.blankError).toBeLessThanOrEqual(2);
      for(const axis of ["left","top","right","bottom"])expect(Math.abs(oracle.actualBounds[axis]-oracle.expectedBounds[axis])).toBeLessThanOrEqual(1);
      for(const pixel of oracle.pixels)expect(pixel.actual).toEqual(pixel.expected);
      const layout=result.texts[0].layout;
      expect(layout.fontSizePx).toBe(30);expect(layout.lineHeightPx).toBe(36);expect(layout.letterSpacingPx).toBe(2);
      expect(layout.block.height).toBe(72);expect(layout.block.visualCenterOffset).toBe(18);
      for(const [index,line] of oracle.lines.entries()){
        expect(layout.lines[index].baselineY).toBe(line.baselineY);
        expect(layout.lines[index].width).toBeCloseTo(line.width,4);
        expect(layout.lines[index].decoration.top).toBeCloseTo(line.baselineY+line.descent+2.1,4);
        expect(layout.lines[index].decoration.height).toBeCloseTo(2.1,4);
      }
      for(const property of ["left","top","width","height","radius"])expect(layout.background[property]).toBeCloseTo(oracle.rectangle[property],4);
      check(`B1 ${family} independent multiline/background/underline/nonuniform-scale/90-degree layout`,{project_id:projectId,revision:fixed.revision,identity:result.identity,font_lock:result.font_lock,execution:result.execution,texts:result.texts,artifact:result.artifact,oracle});
    }
  } finally {
    if(changed){
      const current=await inspect();
      await tool("opencut_project_v4_restore",{project_id:projectId,expected_revision:current.revision,target_revision:original.revision,idempotency_key:randomUUID(),asset_versions:{}});
      const restored=await inspect();
      expect(restored.project).toEqual(original.project);
      await fs.writeFile(metadata,JSON.stringify({project_id:projectId,original_revision:original.revision,restored_revision:restored.revision,status:"restored"})+"\n");
      await page.reload();
      check("B1 layout acceptance restores the original project content through the public revision use case",{project_id:projectId,original_revision:original.revision,restored_revision:restored.revision});
    }
  }
}
