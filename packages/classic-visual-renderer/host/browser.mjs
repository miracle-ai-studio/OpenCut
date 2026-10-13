import { moduleRegistry, playwrightRuntime } from "./modules.mjs";
import { observeVisualPage, executeVisualPage, executeTextVisualPage } from "./page.mjs";
import { checkAbort, integer, natural, reject, sameJsonFacts, VisualExecutionError } from "../src/validation.mjs";
import { fontResource } from "../src/text-frame.mjs";
export const browserHooks={moduleRegistry,playwrightRuntime,
  async configurePage(realm) {
    const textFrame=realm.begin.type==="beginTextFrameV2",plan=textFrame?realm.begin.plan.media:realm.begin.plan;
    const handles=new Map(realm.begin.resources.map(r=>[r.assetId,r.handle])),fixed=new Map(plan.resources.map(r=>[r.assetId,r]));
    const layers=new Set(plan.layers.map(l=>l.elementId));
    const fontHandles=new Map(textFrame?realm.begin.fonts.map(f=>[f.fontId,f.handle]):[]);
    const fonts=new Map(textFrame?realm.begin.plan.texts.map(layer=>[layer.font.fontId,fontResource(layer.font)]):[]);
    const texts=new Set(textFrame?realm.begin.plan.texts.map(layer=>layer.elementId):[]);
    await realm.page.exposeFunction("__opencutVisualCallback",(op,args)=>{
      checkAbort(realm.signal);
      if(!["resolveVerifiedBytes","readVerifiedBytes","releaseVerifiedBytes","admitDecode","bindFrame",...(textFrame?["resolveVerifiedFontBytes","bindTextFrame"]:[])].includes(op)||!args||typeof args!=="object")reject("INVALID_HOST_CALLBACK","Unknown visual callback");
      if(op==="resolveVerifiedBytes") {
        const resource=fixed.get(args.resource?.assetId);
        if(!resource||["contentHash","size","kind"].some(key=>resource[key]!==args.resource[key]))reject("NOT_AUTHORIZED","Visual resource is outside fixed plan");
        args={resource,handle:handles.get(resource.assetId)};
      }
      if(op==="resolveVerifiedFontBytes") {
        const font=fonts.get(args.font?.fontId);
        if(!font||!sameJsonFacts(font,args.font))reject("NOT_AUTHORIZED","Font resource is outside fixed text plan");
        args={font,handle:fontHandles.get(font.fontId)};
      }
      if(op==="admitDecode"&&!layers.has(args.elementId))reject("NOT_AUTHORIZED","Visual element is outside fixed plan");
      if(op==="readVerifiedBytes"){natural(args.offset,"resource offset");integer(args.count,"resource chunk",1,65536);}
      if(op==="bindFrame"&&(!Array.isArray(args.decoded)||args.decoded.length!==layers.size))reject("INVALID_VISUAL_PLAN","Decoded source collection is incomplete");
      if(op==="bindTextFrame"&&(!Array.isArray(args.rasters)||args.rasters.length!==texts.size||new Set(args.rasters.map(r=>r.elementId)).size!==texts.size||args.rasters.some(r=>!texts.has(r.elementId))))reject("INVALID_VISUAL_PLAN","Text raster collection is incomplete");
      return realm.broker.request(op,args);
    });
    await realm.page.exposeFunction("__opencutVisualRgba",async packet=>{
      checkAbort(realm.signal);natural(packet.offsetBytes,"RGBA offset");
      if(typeof packet.bytesHex!=="string"||packet.bytesHex.length===0||packet.bytesHex.length>131072||packet.bytesHex.length%8!==0||!/^(?:[a-f0-9]{2})+$/.test(packet.bytesHex))reject("INVALID_OUTPUT_RGBA","RGBA chunk is invalid");
      await realm.writer.send({...packet,type:"rgba"});
    });
  },
  async observeRuntime(realm,modules) {
    const gpu=await realm.page.evaluate(observeVisualPage);checkAbort(realm.signal);
    if(gpu.gpuBackend!=="webgpu"||gpu.textureFormat!=="bgra8unorm"||typeof gpu.gpuDevice!=="string"||!gpu.gpuDevice)reject("VISUAL_GPU_FAILED","Actual GPU identity was not observed");
    return {...gpu,shaderSha256:modules.shaderSha256,decoderProfile:"mediabunny-1.41.0-initial-canvases-half-open;static-png-jpeg-browser-image",engine:realm.identity.engine+" · "+modules.shaderSha256+" · "+gpu.gpuDevice};
  },
  async render(realm) {
    const execute=realm.begin.type==="beginTextFrameV2"?executeTextVisualPage:executeVisualPage;
    const result=await realm.page.evaluate(execute,{plan:realm.begin.plan,admission:realm.begin.admission});
    if(result?.ok!==true)throw new VisualExecutionError(result?.error?.code??"VISUAL_BROWSER_EXECUTION_FAILED","Browser visual execution failed");
    return result.receipt;
  }
};
