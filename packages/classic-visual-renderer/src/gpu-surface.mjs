import {reject} from "./validation.mjs";
const frames=new WeakMap();
const exportSurfaces=new WeakMap();
/** Private same-device frame ownership; never serializable or cross-realm. */
export function captureGpuSurfaceFrame(realm,encoder,source,width,height) {
  const texture=realm.device.createTexture({size:[width,height],format:"bgra8unorm",usage:GPUTextureUsage.COPY_DST|GPUTextureUsage.COPY_SRC});
  let resolve,rejectReady;
  const ready=new Promise((yes,no)=>{resolve=yes;rejectReady=no;});void ready.catch(()=>{});
  const state={realm,texture,closed:false,validated:false,resolve,rejectReady};
  const frame=Object.freeze({width,height,ready,close(){if(state.closed)return;state.closed=true;texture.destroy();}});
  frames.set(frame,state);
  try {encoder.copyTextureToTexture({texture:source},{texture},[width,height]);return frame;}
  catch(error){frame.close();throw error;}
}
export function validateGpuSurfaceFrame(frame,validation){
  const state=frames.get(frame);if(!state)reject("VISUAL_GPU_FAILED","Unknown owned GPU frame");
  void validation.then(()=>{if(!state.closed)state.validated=true;state.resolve();},error=>{frame.close();state.rejectReady(error);});
}
/** Export only a validated frame to another execution worker. */
export async function gpuSurfaceBitmap(realm,frame,canvas){
  const state=frames.get(frame);
  if(!state||state.realm!==realm||state.closed||!state.validated||realm.closed)reject("VISUAL_GPU_FAILED","GPU frame is not transferable after validation");
  let retained=exportSurfaces.get(canvas);
  if(!retained||retained.realm!==realm||canvas.width!==frame.width||canvas.height!==frame.height){
    retained?.painter.clear();canvas.width=frame.width;canvas.height=frame.height;
    retained={realm,painter:gpuSurfacePresenter(realm,canvas)};exportSurfaces.set(canvas,retained);
  }
  realm.device.pushErrorScope('out-of-memory');realm.device.pushErrorScope('validation');
  let bitmap;
  try {retained.painter.draw(frame);bitmap=canvas.transferToImageBitmap();}
  finally {
    const validation=realm.device.popErrorScope(),memory=realm.device.popErrorScope();
    const errors=await Promise.all([validation,memory]);
    if(errors.some(Boolean)||realm.closed){bitmap?.close();reject('VISUAL_GPU_FAILED','GPU rejected transferable picture');}
  }
  return bitmap;
}
/** Present an already validated bitmap on the renderer's owned GPU device. */
export function gpuSurfacePresenter(realm,canvas) {
  if(!realm||realm.closed||!realm.device||realm.observation.textureFormat!=="bgra8unorm")reject("VISUAL_GPU_FAILED","Presentation GPU realm unavailable");
  const context=canvas.getContext("webgpu");
  if(!context)reject("CAPABILITY_UNAVAILABLE","GPU presentation canvas unavailable");
  context.configure({device:realm.device,format:"bgra8unorm",alphaMode:"opaque",usage:GPUTextureUsage.COPY_DST|GPUTextureUsage.RENDER_ATTACHMENT});
  let closed=false;
  return {
    draw(bitmap){
      if(closed||realm.closed||bitmap.width!==canvas.width||bitmap.height!==canvas.height)reject("VISUAL_GPU_FAILED","Presentation bitmap changed its admitted raster");
      const owned=frames.get(bitmap);
      if(owned){
        if(owned.closed||!owned.validated||owned.realm!==realm)reject("VISUAL_GPU_FAILED","Presentation texture changed its owning realm or is not validated");
        const encoder=realm.device.createCommandEncoder();
        encoder.copyTextureToTexture({texture:owned.texture},{texture:context.getCurrentTexture()},[canvas.width,canvas.height]);
        realm.device.queue.submit([encoder.finish()]);
      }else realm.device.queue.copyExternalImageToTexture({source:bitmap,flipY:false},{texture:context.getCurrentTexture(),colorSpace:"srgb",premultipliedAlpha:false},[canvas.width,canvas.height]);
    },
    clear(){if(closed)return;closed=true;context.unconfigure();canvas.width=0;canvas.height=0;}
  };
}
