import { Input, BlobSource, ALL_FORMATS, CanvasSink } from "mediabunny";
import { staticImageHeader, admitDimensions } from "./image-format.mjs";
import { checkAbort, finite, reject } from "./validation.mjs";
function canvasCopy(source,width,height) {
  const canvas = new OffscreenCanvas(width,height), context = canvas.getContext("2d");
  if (!context) reject("VISUAL_DECODE_FAILED","Decoded source canvas is unavailable");
  context.drawImage(source,0,0,width,height);return canvas;
}
export async function decodeImage(bytes,admission,signal) {
  const header=staticImageHeader(bytes);admitDimensions(header.width,header.height,admission);checkAbort(signal);
  const image=new Image(),url=URL.createObjectURL(new Blob([bytes],{type:header.mime}));
  try {
    await new Promise((resolve,fail)=>{image.onload=resolve;image.onerror=()=>fail(new Error("Static image decode failed"));image.src=url;});
    checkAbort(signal);admitDimensions(image.naturalWidth,image.naturalHeight,admission);
    return {canvas:canvasCopy(image,image.naturalWidth,image.naturalHeight),width:image.naturalWidth,height:image.naturalHeight};
  } catch(error) { if(error.code) throw error;reject("VISUAL_DECODE_FAILED","Static image could not be decoded"); }
  finally {image.onload=null;image.onerror=null;image.src="";URL.revokeObjectURL(url);}
}
export async function decodeVideo(bytes,decode,admission,signal) {
  checkAbort(signal);const requested=finite(decode.requestedTimeSeconds,"observed decode request");
  if (decode.selectedVideoStream !== null) reject("UNSUPPORTED_RENDER_FEATURE","Only primary video selection is supported");
  const input=new Input({source:new BlobSource(new Blob([bytes])),formats:ALL_FORMATS});let iterator;
  try {
    const tracks=await input.getVideoTracks();checkAbort(signal);
    if(tracks.length!==1) reject("UNSUPPORTED_RENDER_FEATURE","Exactly one video stream is required");
    const track=await input.getPrimaryVideoTrack();
    if(!track) reject("VISUAL_DECODE_FAILED","Primary video track is unavailable");
    // The pinned decoder can silently continue with color-only output if its
    // separate-alpha merger cannot initialize. This profile has no independently
    // verified transparent-video path, so refuse before decoder allocation.
    if(await track.canBeTransparent()) reject("UNSUPPORTED_RENDER_FEATURE","Transparent video is not supported by this execution profile");
    if(!await track.canDecode()) reject("VISUAL_DECODE_FAILED","Primary video codec is unavailable");
    // Admit actual probed display dimensions before allocating a CanvasSink.
    admitDimensions(track.displayWidth,track.displayHeight,admission);
    const sink=new CanvasSink(track,{poolSize:3,fit:"contain"});iterator=sink.canvases(requested);
    const item=await iterator.next();checkAbort(signal);
    if(item.done||!item.value) return {reason:"eof"};
    const value=item.value,timestamp=finite(value.timestamp,"decoded PTS"),duration=finite(value.duration,"decoded frame duration");
    if(duration<=0||!Number.isFinite(timestamp+duration)) reject("VISUAL_DECODE_FAILED","Decoded video interval is invalid");
    if(timestamp>requested||requested>=timestamp+duration) return {reason:"timestamp_gap"};
    const width=value.canvas.width,height=value.canvas.height;admitDimensions(width,height,admission);
    return {canvas:canvasCopy(value.canvas,width,height),width,height,actualTimestampSeconds:timestamp,frameDurationSeconds:duration};
  } catch(error) {if(error.code)throw error;reject("VISUAL_DECODE_FAILED","Video decoder failed");}
  finally {try{await iterator?.return();}finally{input.dispose();}}
}
