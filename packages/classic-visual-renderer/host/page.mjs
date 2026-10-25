// Serialized into the isolated realm; imports only its fixed module registry.
export async function observeVisualPage() {
  const module=await import("/renderer/src/index.mjs");
  globalThis.__opencutVisualModule=module;globalThis.__opencutVisualGpu=await module.openGpuRealm();
  return globalThis.__opencutVisualGpu.observation;
}
export async function executeVisualPage({plan,admission}) {
  try {
    const host={admission,callback:(op,args)=>globalThis.__opencutVisualCallback(op,args),rgba:async(offset,bytes)=>{
      const hex=globalThis.__opencutVisualModule.encodeRgbaHex(bytes);
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
      const hex=globalThis.__opencutVisualModule.encodeRgbaHex(bytes);
      await globalThis.__opencutVisualRgba({offsetBytes:String(offset),bytesHex:hex});
    }};
    const receipt=await globalThis.__opencutVisualModule.renderClassicTextFrame(plan,host,globalThis.__opencutVisualGpu);
    return {ok:true,receipt};
  } catch(error) {return {ok:false,error:{code:error.code??"VISUAL_BROWSER_EXECUTION_FAILED"}};}
  finally {if(globalThis.__opencutVisualGpu)globalThis.__opencutVisualModule.closeGpuRealm(globalThis.__opencutVisualGpu);}
}

// Separate serializable V3 entry; all byte reads and RGBA transport are shared.
export async function executeCaptionVisualPage({plan,admission}) {
  try {
    const host={admission,callback:(op,args)=>globalThis.__opencutVisualCallback(op,args),rgba:async(offset,bytes)=>{
      const hex=globalThis.__opencutVisualModule.encodeRgbaHex(bytes);
      await globalThis.__opencutVisualRgba({offsetBytes:String(offset),bytesHex:hex});
    }};
    const {renderClassicCaptionFrame}=await import("/renderer/src/caption-frame.mjs");
    const receipt=await renderClassicCaptionFrame(plan,host,globalThis.__opencutVisualGpu);
    return {ok:true,receipt};
  } catch(error) {return {ok:false,error:{code:error.code??"VISUAL_BROWSER_EXECUTION_FAILED"}};}
  finally {if(globalThis.__opencutVisualGpu)globalThis.__opencutVisualModule.closeGpuRealm(globalThis.__opencutVisualGpu);}
}

export async function executeGraphicVisualPage({plan,admission}) {
  try {
    const host={admission,callback:(op,args)=>globalThis.__opencutVisualCallback(op,args),rgba:async(offset,bytes)=>{
      const hex=globalThis.__opencutVisualModule.encodeRgbaHex(bytes);
      await globalThis.__opencutVisualRgba({offsetBytes:String(offset),bytesHex:hex});
    }};
    const {renderClassicGraphicFrame}=await import("/renderer/src/graphic-frame.mjs");
    const receipt=await renderClassicGraphicFrame(plan,host,globalThis.__opencutVisualGpu);
    return {ok:true,receipt};
  } catch(error) {return {ok:false,error:{code:error.code??"VISUAL_BROWSER_EXECUTION_FAILED"}};}
  finally {if(globalThis.__opencutVisualGpu)globalThis.__opencutVisualModule.closeGpuRealm(globalThis.__opencutVisualGpu);}
}

export async function executeBlendVisualPage({plan,admission}) {
  try {
    const host={admission,callback:(op,args)=>globalThis.__opencutVisualCallback(op,args),rgba:async(offset,bytes)=>{
      const hex=globalThis.__opencutVisualModule.encodeRgbaHex(bytes);
      await globalThis.__opencutVisualRgba({offsetBytes:String(offset),bytesHex:hex});
    }};
    const {renderClassicBlendFrame}=await import("/renderer/src/blend-frame.mjs");
    const receipt=await renderClassicBlendFrame(plan,host,globalThis.__opencutVisualGpu);
    return {ok:true,receipt};
  } catch(error) {return {ok:false,error:{code:error.code??"VISUAL_BROWSER_EXECUTION_FAILED"}};}
  finally {if(globalThis.__opencutVisualGpu)globalThis.__opencutVisualModule.closeGpuRealm(globalThis.__opencutVisualGpu);}
}

export async function executeMaskVisualPage({plan,admission}) {
  try {
    const host={admission,callback:(op,args)=>globalThis.__opencutVisualCallback(op,args),rgba:async(offset,bytes)=>{
      const hex=globalThis.__opencutVisualModule.encodeRgbaHex(bytes);
      await globalThis.__opencutVisualRgba({offsetBytes:String(offset),bytesHex:hex});
    }};
    const {renderClassicMaskFrame}=await import("/renderer/src/mask-frame.mjs");
    const receipt=await renderClassicMaskFrame(plan,host,globalThis.__opencutVisualGpu);
    return {ok:true,receipt};
  } catch(error) {return {ok:false,error:{code:error.code??"VISUAL_BROWSER_EXECUTION_FAILED"}};}
  finally {if(globalThis.__opencutVisualGpu)globalThis.__opencutVisualModule.closeGpuRealm(globalThis.__opencutVisualGpu);}
}

export async function executeSceneVisualPage({plan,admission}) {
  try {
    const host={admission,callback:(op,args)=>globalThis.__opencutVisualCallback(op,args),rgba:async(offset,bytes)=>{
      const hex=globalThis.__opencutVisualModule.encodeRgbaHex(bytes);
      await globalThis.__opencutVisualRgba({offsetBytes:String(offset),bytesHex:hex});
    }};
    const {renderClassicSceneFrame}=await import("/renderer/src/scene-frame.mjs");
    const receipt=await renderClassicSceneFrame(plan,host,globalThis.__opencutVisualGpu);
    return {ok:true,receipt};
  } catch(error) {return {ok:false,error:{code:error.code??"VISUAL_BROWSER_EXECUTION_FAILED"}};}
  finally {if(globalThis.__opencutVisualGpu)globalThis.__opencutVisualModule.closeGpuRealm(globalThis.__opencutVisualGpu);}
}
