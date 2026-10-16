// Private persistent execution adapter. Layout/decoding/composition remain in
// the same versioned frame renderers used by PNG and live preview.
import { browserHooks } from "./browser.mjs";
import { checkAbort, reject } from "../src/validation.mjs";
export const videoBrowserHooks={...browserHooks,
  async configurePage(realm) {
    let activeFrame,configured;
    // Capture the ordinary single-frame handlers instead of reimplementing
    // their resource authorization and text/caption/graphic binding fences.
    // Only the handlers reset per frame; browser, decoder and font caches persist.
    const invoke=async(name,args)=>{
      checkAbort(realm.signal);const frame=realm.currentFrame;
      if(!frame)reject("INVALID_HOST_CALLBACK","Callback outside active video frame");
      if(frame!==activeFrame) {
        activeFrame=frame;const handlers=new Map();
        configured=browserHooks.configurePage({...realm,begin:frame,
          page:{exposeFunction:async(key,handler)=>handlers.set(key,handler)},
          writer:{send:packet=>realm.writer.send({...packet,frameIndex:frame.frameIndex})},
        }).then(()=>handlers);
      }
      const handlers=await configured;checkAbort(realm.signal);
      if(realm.currentFrame!==frame)reject("INVALID_HOST_CALLBACK","Video frame ended before callback admission");
      return handlers.get(name)(...args);
    };
    for(const name of ["__opencutVisualCallback","__opencutVisualRgba"])
      await realm.page.exposeFunction(name,(...args)=>invoke(name,args));
  },
  async render(realm) {
    const result=await realm.page.evaluate(executeVideoFrame,{plan:realm.currentFrame.plan,admission:realm.begin.admission,profile:realm.currentFrame.type});
    if(result?.ok!==true)reject(result?.error?.code??"VISUAL_BROWSER_EXECUTION_FAILED","Persistent video frame failed");
    return result.receipt;
  }
};
export async function executeVideoFrame({plan,admission,profile="beginTextFrameV2"}) {
  try {
    const module=globalThis.__opencutVisualModule;
    globalThis.__opencutVideoBytes??=new Map();
    globalThis.__opencutVideoFonts??=new Map();
    globalThis.__opencutVideoDecoders??=new module.VisualDecoderCache();
    const host={admission,requireResolveCached:true,callback:(op,args)=>globalThis.__opencutVisualCallback(op,args),rgba:async(offset,bytes)=>{
      let hex="";for(const byte of bytes)hex+=byte.toString(16).padStart(2,"0");
      await globalThis.__opencutVisualRgba({offsetBytes:String(offset),bytesHex:hex});
    }};
    const render={beginTextFrameV2:module.renderClassicTextFrameV2,beginCaptionFrameV3:module.renderClassicCaptionFrameV3,beginGraphicFrameV1:module.renderClassicGraphicFrameV1,beginBlendFrameV1:module.renderClassicBlendFrameV1,beginMaskFrameV1:module.renderClassicMaskFrameV1,beginSceneFrameV1:module.renderClassicSceneFrameV1}[profile];
    if(typeof render!=="function")return {ok:false,error:{code:"INVALID_HOST_PROTOCOL"}};
    const receipt=await render(plan,host,globalThis.__opencutVisualGpu,undefined,globalThis.__opencutVideoBytes,globalThis.__opencutVideoDecoders,globalThis.__opencutVideoFonts);
    return {ok:true,receipt};
  } catch(error) {return {ok:false,error:{code:error.code??"VISUAL_BROWSER_EXECUTION_FAILED"}};}
}
