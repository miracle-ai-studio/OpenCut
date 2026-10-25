import { graphicSourceLayout } from "../src/graphic-source.mjs";
import { moduleRegistry, playwrightRuntime } from "./modules.mjs";
import { observeVisualPage, executeVisualPage, executeTextVisualPage, executeCaptionVisualPage, executeGraphicVisualPage, executeBlendVisualPage, executeMaskVisualPage, executeSceneVisualPage } from "./page.mjs";
import { checkAbort, integer, natural, reject, sameJsonFacts, VisualExecutionError } from "../src/validation.mjs";
import { sceneFontResources } from "../src/scene-text.mjs";
import { fontResource } from "../src/text-frame.mjs";
export const browserHooks={moduleRegistry,playwrightRuntime,ownedLocal:true,
  async configurePage(realm) {
    const sceneFrame=realm.begin.type==="beginSceneFrame",maskFrame=realm.begin.type==="beginMaskFrame",blendFrame=sceneFrame||maskFrame||realm.begin.type==="beginBlendFrame",graphicFrame=blendFrame||realm.begin.type==="beginGraphicFrame",captionFrame=graphicFrame||realm.begin.type==="beginCaptionFrame",textFrame=captionFrame||realm.begin.type==="beginTextFrame";
    const blendPlan=sceneFrame||maskFrame?realm.begin.plan.execution:realm.begin.plan;
    const graphicPlan=blendFrame?blendPlan.execution:realm.begin.plan;
    const captionPlan=graphicFrame?graphicPlan.execution:realm.begin.plan;
    const textPlan=captionFrame?captionPlan.execution:realm.begin.plan,plan=textFrame?textPlan.media:realm.begin.plan;
    const handles=new Map(realm.begin.resources.map(r=>[r.assetId,r.handle])),fixed=new Map(plan.resources.map(r=>[r.assetId,r]));
    const layers=new Set(plan.layers.map(l=>l.elementId));
    const fontHandles=new Map(textFrame?realm.begin.fonts.map(f=>[f.fontId,f.handle]):[]);
    const fonts=sceneFrame?sceneFontResources(realm.begin.plan):new Map(textFrame?textPlan.texts.map(layer=>[layer.font.fontId,fontResource(layer.font)]):[]);
    const texts=new Set(textFrame?textPlan.texts.map(layer=>layer.elementId):[]);
    const captions=new Map(captionFrame?captionPlan.captions.map(c=>[c.elementId,c]):[]);
    const expectedGraphics=graphicFrame?graphicPlan.graphics.map(layer=>{
      const {width,height,originX,originY}=graphicSourceLayout(layer.graphic,{maxPixels:realm.begin.admission.maxDecodedPixels,maxRasterBytes:realm.begin.admission.maxRgbaBytes});
      return {elementId:layer.elementId,width,height,originX,originY};
    }):[];
    let captionBound=false,graphicBound=false,captionBinding=false,graphicBinding=false,blendBound=false,blendBinding=false,maskBound=false,maskBinding=false;
    await realm.page.exposeFunction("__opencutVisualCallback",(op,args)=>{
      checkAbort(realm.signal);
      if(!["resolveVerifiedBytes","readVerifiedBytes","releaseVerifiedBytes","admitDecode","bindFrame",...(textFrame?["resolveVerifiedFontBytes",captionFrame?"bindCaptionFrameV3":"bindTextFrame"]:[]),...(graphicFrame?["bindGraphicFrameV1"]:[]),...(blendFrame?["bindBlendFrameV1"]:[]),...(maskFrame?["bindMaskFrameV1"]:[]),...(sceneFrame?["bindSceneFrameV1"]:[])].includes(op)||!args||typeof args!=="object")reject("INVALID_HOST_CALLBACK","Unknown visual callback");
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
      if(op==="bindCaptionFrameV3") {
        if(!Array.isArray(args.rasters)||args.rasters.length!==texts.size||new Set(args.rasters.map(r=>r.elementId)).size!==texts.size||args.rasters.some(r=>!texts.has(r.elementId)))reject("INVALID_VISUAL_PLAN","V3 text raster collection is incomplete");
        if(!Array.isArray(args.captions)||args.captions.length!==captions.size||new Set(args.captions.map(c=>c.elementId)).size!==captions.size||args.captions.some(c=>!captions.has(c.elementId)))reject("INVALID_VISUAL_PLAN","V3 caption observation collection is incomplete");
        for(const caption of args.captions) {
          const original=textPlan.texts.find(layer=>layer.elementId===caption.elementId);
          if(caption.originalTextHash!==original.font.validatedTextHash||typeof caption.wrappedText!=="string"||!Array.isArray(caption.measurements))reject("INVALID_VISUAL_PLAN","V3 caption observations changed fixed original identity");
        }
      }
      if(graphicFrame && op==="bindCaptionFrameV3") {
        if(captionBinding||captionBound||graphicBound)reject("INVALID_HOST_CALLBACK","Graphic child already bound");
        captionBinding=true;return Promise.resolve(realm.broker.request(op,args)).then(result=>{captionBound=true;return result;});
      }
      if(op==="bindGraphicFrameV1") {
        if(!captionBound||graphicBinding||graphicBound||!sameJsonFacts(args.graphics,expectedGraphics))reject("INVALID_VISUAL_PLAN","Graphic observations require one complete child binding and fixed raster sizes");
        graphicBinding=true;return Promise.resolve(realm.broker.request(op,args)).then(result=>{graphicBound=true;return result;});
      }
      if(op==="bindBlendFrameV1") {
        if(!graphicBound||blendBinding||blendBound||!sameJsonFacts(args,{}))reject("INVALID_VISUAL_PLAN","Blend binding requires one completed graphic binding and no browser-authored modes");
        blendBinding=true;return Promise.resolve(realm.broker.request(op,args)).then(result=>{blendBound=true;return result;});
      }
      if(op==="bindMaskFrameV1"||op==="bindSceneFrameV1") {
        if(!blendBound||maskBinding||maskBound||!sameJsonFacts(args,{}))reject("INVALID_VISUAL_PLAN","Mask binding requires completed blend binding and no browser-authored masks");
        maskBinding=true;return Promise.resolve(realm.broker.request(op,args)).then(result=>{maskBound=true;return result;});
      }
      return realm.broker.request(op,args);
    });
    await realm.page.exposeFunction("__opencutVisualRgba",async packet=>{
      checkAbort(realm.signal);natural(packet.offsetBytes,"RGBA offset");
      if((maskFrame||sceneFrame)&&!maskBound)reject("INVALID_VISUAL_PLAN","Mask pixels require final fixed binding");
      if(blendFrame&&!blendBound)reject("INVALID_VISUAL_PLAN","Blend pixels require the final fixed binding");
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
    const execute=realm.begin.type==="beginSceneFrame"?executeSceneVisualPage:realm.begin.type==="beginMaskFrame"?executeMaskVisualPage:realm.begin.type==="beginBlendFrame"?executeBlendVisualPage:realm.begin.type==="beginGraphicFrame"?executeGraphicVisualPage:realm.begin.type==="beginCaptionFrame"?executeCaptionVisualPage:realm.begin.type==="beginTextFrame"?executeTextVisualPage:executeVisualPage;
    const result=await realm.page.evaluate(execute,{plan:realm.begin.plan,admission:realm.begin.admission});
    if(result?.ok!==true)throw new VisualExecutionError(result?.error?.code??"VISUAL_BROWSER_EXECUTION_FAILED","Browser visual execution failed");
    return result.receipt;
  }
};
