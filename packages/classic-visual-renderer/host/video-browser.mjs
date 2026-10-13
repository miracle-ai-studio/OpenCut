// Private persistent execution adapter. Layout/decoding/composition remain in
// the exact V2 renderer used by PNG and live preview.
import { browserHooks } from "./browser.mjs";
import { checkAbort, natural, reject, sameJsonFacts } from "../src/validation.mjs";
import { fontResource } from "../src/text-frame.mjs";
export const videoBrowserHooks={...browserHooks,
  async configurePage(realm) {
    await realm.page.exposeFunction("__opencutVisualCallback",(op,args)=>{
      checkAbort(realm.signal);const frame=realm.currentFrame;
      const caption = realm.begin.type === "beginCaptionVideoV2";
      const plan = caption ? frame?.plan.execution : frame?.plan;
      if(!frame||!["resolveVerifiedBytes","readVerifiedBytes","releaseVerifiedBytes","admitDecode","bindFrame","resolveVerifiedFontBytes",caption ? "bindCaptionFrameV3" : "bindTextFrame"].includes(op)||!args||typeof args!=="object")reject("INVALID_HOST_CALLBACK","Callback outside active video frame");
      if(op==="resolveVerifiedBytes") {
        const resource=plan.media.resources.find(r=>r.assetId===args.resource?.assetId);
        if(!resource||!sameJsonFacts(resource,args.resource))reject("NOT_AUTHORIZED","Video resource differs from fixed frame");
        args={resource,handle:frame.resources.find(r=>r.assetId===resource.assetId)?.handle};
      }
      if(op==="resolveVerifiedFontBytes") {
        const layer=plan.texts.find(t=>t.font.fontId===args.font?.fontId),font=layer&&fontResource(layer.font);
        if(!font||!sameJsonFacts(font,args.font))reject("NOT_AUTHORIZED","Video font differs from fixed frame");
        args={font,handle:frame.fonts.find(f=>f.fontId===font.fontId)?.handle};
      }
      if(op==="admitDecode"&&!plan.media.layers.some(l=>l.elementId===args.elementId))reject("NOT_AUTHORIZED","Video layer differs from fixed frame");
      return realm.broker.request(op,args);
    });
    await realm.page.exposeFunction("__opencutVisualRgba",async packet=>{
      checkAbort(realm.signal);if(!realm.currentFrame)reject("INVALID_HOST_PROTOCOL","RGBA outside active video frame");
      natural(packet.offsetBytes,"RGBA offset");
      if(typeof packet.bytesHex!=="string"||!packet.bytesHex.length||packet.bytesHex.length>131072||packet.bytesHex.length%8||!/^(?:[a-f0-9]{2})+$/.test(packet.bytesHex))reject("INVALID_OUTPUT_RGBA","Invalid bounded video RGBA");
      await realm.writer.send({...packet,type:"rgba",frameIndex:realm.currentFrame.frameIndex});
    });
  },
  async render(realm) {
    const result=await realm.page.evaluate(executeVideoFrame,{plan:realm.currentFrame.plan,admission:realm.begin.admission,caption:realm.begin.type === "beginCaptionVideoV2"});
    if(result?.ok!==true)reject(result?.error?.code??"VISUAL_BROWSER_EXECUTION_FAILED","Persistent video frame failed");
    return result.receipt;
  }
};
export async function executeVideoFrame({plan,admission,caption=false}) {
  try {
    const module=globalThis.__opencutVisualModule;
    globalThis.__opencutVideoBytes??=new Map();
    globalThis.__opencutVideoFonts??=new Map();
    globalThis.__opencutVideoDecoders??=new module.VisualDecoderCache();
    const host={admission,requireResolveCached:true,callback:(op,args)=>globalThis.__opencutVisualCallback(op,args),rgba:async(offset,bytes)=>{
      let hex="";for(const byte of bytes)hex+=byte.toString(16).padStart(2,"0");
      await globalThis.__opencutVisualRgba({offsetBytes:String(offset),bytesHex:hex});
    }};
    const render = caption ? module.renderClassicCaptionFrameV3 : module.renderClassicTextFrameV2;
    const receipt=await render(plan,host,globalThis.__opencutVisualGpu,undefined,globalThis.__opencutVideoBytes,globalThis.__opencutVideoDecoders,globalThis.__opencutVideoFonts);
    return {ok:true,receipt};
  } catch(error) {return {ok:false,error:{code:error.code??"VISUAL_BROWSER_EXECUTION_FAILED"}};}
}
