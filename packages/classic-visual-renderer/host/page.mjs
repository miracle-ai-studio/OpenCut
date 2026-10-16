// Serialized into the isolated realm; imports only its fixed module registry.
export async function observeVisualPage() {
  const module=await import("/renderer/src/index.mjs");
  globalThis.__opencutVisualModule=module;globalThis.__opencutVisualGpu=await module.openGpuRealm();
  return globalThis.__opencutVisualGpu.observation;
}
export async function executeVisualPage({plan,admission}) {
  try {
    const host={admission,callback:(op,args)=>globalThis.__opencutVisualCallback(op,args),rgba:async(offset,bytes)=>{
      let hex="";for(const byte of bytes)hex+=byte.toString(16).padStart(2,"0");
      await globalThis.__opencutVisualRgba({offsetBytes:String(offset),bytesHex:hex});
    }};
    const receipt=await globalThis.__opencutVisualModule.renderClassicFrame(plan,host,globalThis.__opencutVisualGpu);
    return {ok:true,receipt};
  } catch(error) {return {ok:false,error:{code:error.code??"VISUAL_BROWSER_EXECUTION_FAILED"}};}
  finally {if(globalThis.__opencutVisualGpu)globalThis.__opencutVisualModule.closeGpuRealm(globalThis.__opencutVisualGpu);}
}

// Kept as a standalone serializable function for the isolated page. Transport
// dispatch selects this only for the new, independently validated operation.
export async function executeTextVisualPage({plan,admission}) {
  try {
    const host={admission,callback:(op,args)=>globalThis.__opencutVisualCallback(op,args),rgba:async(offset,bytes)=>{
      let hex="";for(const byte of bytes)hex+=byte.toString(16).padStart(2,"0");
      await globalThis.__opencutVisualRgba({offsetBytes:String(offset),bytesHex:hex});
    }};
    const receipt=await globalThis.__opencutVisualModule.renderClassicTextFrameV2(plan,host,globalThis.__opencutVisualGpu);
    return {ok:true,receipt};
  } catch(error) {return {ok:false,error:{code:error.code??"VISUAL_BROWSER_EXECUTION_FAILED"}};}
  finally {if(globalThis.__opencutVisualGpu)globalThis.__opencutVisualModule.closeGpuRealm(globalThis.__opencutVisualGpu);}
}

// Separate serializable V3 entry; all byte reads and RGBA transport are shared.
export async function executeCaptionVisualPageV3({plan,admission}) {
  try {
    const host={admission,callback:(op,args)=>globalThis.__opencutVisualCallback(op,args),rgba:async(offset,bytes)=>{
      let hex="";for(const byte of bytes)hex+=byte.toString(16).padStart(2,"0");
      await globalThis.__opencutVisualRgba({offsetBytes:String(offset),bytesHex:hex});
    }};
    const {renderClassicCaptionFrameV3}=await import("/renderer/src/text-frame-v3.mjs");
    const receipt=await renderClassicCaptionFrameV3(plan,host,globalThis.__opencutVisualGpu);
    return {ok:true,receipt};
  } catch(error) {return {ok:false,error:{code:error.code??"VISUAL_BROWSER_EXECUTION_FAILED"}};}
  finally {if(globalThis.__opencutVisualGpu)globalThis.__opencutVisualModule.closeGpuRealm(globalThis.__opencutVisualGpu);}
}

export async function executeGraphicVisualPageV1({plan,admission}) {
  try {
    const host={admission,callback:(op,args)=>globalThis.__opencutVisualCallback(op,args),rgba:async(offset,bytes)=>{
      let hex="";for(const byte of bytes)hex+=byte.toString(16).padStart(2,"0");
      await globalThis.__opencutVisualRgba({offsetBytes:String(offset),bytesHex:hex});
    }};
    const {renderClassicGraphicFrameV1}=await import("/renderer/src/graphic-frame-v1.mjs");
    const receipt=await renderClassicGraphicFrameV1(plan,host,globalThis.__opencutVisualGpu);
    return {ok:true,receipt};
  } catch(error) {return {ok:false,error:{code:error.code??"VISUAL_BROWSER_EXECUTION_FAILED"}};}
  finally {if(globalThis.__opencutVisualGpu)globalThis.__opencutVisualModule.closeGpuRealm(globalThis.__opencutVisualGpu);}
}

export async function executeBlendVisualPageV1({plan,admission}) {
  try {
    const host={admission,callback:(op,args)=>globalThis.__opencutVisualCallback(op,args),rgba:async(offset,bytes)=>{
      let hex="";for(const byte of bytes)hex+=byte.toString(16).padStart(2,"0");
      await globalThis.__opencutVisualRgba({offsetBytes:String(offset),bytesHex:hex});
    }};
    const {renderClassicBlendFrameV1}=await import("/renderer/src/blend-frame-v1.mjs");
    const receipt=await renderClassicBlendFrameV1(plan,host,globalThis.__opencutVisualGpu);
    return {ok:true,receipt};
  } catch(error) {return {ok:false,error:{code:error.code??"VISUAL_BROWSER_EXECUTION_FAILED"}};}
  finally {if(globalThis.__opencutVisualGpu)globalThis.__opencutVisualModule.closeGpuRealm(globalThis.__opencutVisualGpu);}
}

export async function executeMaskVisualPageV1({plan,admission}) {
  try {
    const host={admission,callback:(op,args)=>globalThis.__opencutVisualCallback(op,args),rgba:async(offset,bytes)=>{
      let hex="";for(const byte of bytes)hex+=byte.toString(16).padStart(2,"0");
      await globalThis.__opencutVisualRgba({offsetBytes:String(offset),bytesHex:hex});
    }};
    const {renderClassicMaskFrameV1}=await import("/renderer/src/mask-frame-v1.mjs");
    const receipt=await renderClassicMaskFrameV1(plan,host,globalThis.__opencutVisualGpu);
    return {ok:true,receipt};
  } catch(error) {return {ok:false,error:{code:error.code??"VISUAL_BROWSER_EXECUTION_FAILED"}};}
  finally {if(globalThis.__opencutVisualGpu)globalThis.__opencutVisualModule.closeGpuRealm(globalThis.__opencutVisualGpu);}
}

export async function executeSceneVisualPageV1({plan,admission}) {
  try {
    const host={admission,callback:(op,args)=>globalThis.__opencutVisualCallback(op,args),rgba:async(offset,bytes)=>{
      let hex="";for(const byte of bytes)hex+=byte.toString(16).padStart(2,"0");
      await globalThis.__opencutVisualRgba({offsetBytes:String(offset),bytesHex:hex});
    }};
    const {renderClassicSceneFrameV1}=await import("/renderer/src/scene-frame-v1.mjs");
    const receipt=await renderClassicSceneFrameV1(plan,host,globalThis.__opencutVisualGpu);
    return {ok:true,receipt};
  } catch(error) {return {ok:false,error:{code:error.code??"VISUAL_BROWSER_EXECUTION_FAILED"}};}
  finally {if(globalThis.__opencutVisualGpu)globalThis.__opencutVisualModule.closeGpuRealm(globalThis.__opencutVisualGpu);}
}
