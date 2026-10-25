// Decode an admitted immutable derived segment. This adapter never samples a
// project, evaluates an effect, retimes a source or runs a presentation clock.
import { Input, ALL_FORMATS, CanvasSink } from "mediabunny";
export class PreviewSegmentDecoder {
  entries=new Map();
  generation=0;
  async open(key,blob,width,height) {
    if(this.entries.has(key))return this.entries.get(key);
    if(this.entries.size>=2)throw Error("Preview segment decoder exceeds its two-segment budget");
    const generation=this.generation;
    const {BlobSource}=await import("mediabunny");
    const input=new Input({source:new BlobSource(blob),formats:ALL_FORMATS});
    const entry={input,iterator:null,current:null,last:-Infinity};
    try {
      const track=await input.getPrimaryVideoTrack();
      if(!track||track.displayWidth!==width||track.displayHeight!==height||!await track.canDecode())throw Error("Invalid preview segment video track");
      entry.sink=new CanvasSink(track,{poolSize:3,fit:"contain"});
      if(generation!==this.generation)throw new DOMException("Preview decoder closed","AbortError");
      this.entries.set(key,entry);
      return entry;
    } catch(error) {input.dispose();throw error;}
  }
  async warm(key,blob,width,height) {
    const entry=await this.open(key,blob,width,height);
    if(!entry.iterator){entry.iterator=entry.sink.canvases(0);const first=await entry.iterator.next();
      if(first.done||!first.value)throw Error("Missing first frame in preview segment");entry.current=first.value;entry.last=0;}
  }
  async retain(keys) {
    for(const [key,entry] of this.entries)if(!keys.has(key)) {
      this.entries.delete(key);try{await entry.iterator?.return();}finally{entry.input.dispose();}
    }
  }
  async frame(key,blob,width,height,at,signal) {
    signal.throwIfAborted();const entry=await this.open(key,blob,width,height);signal.throwIfAborted();
    if(!entry.iterator||at<entry.last) {
      await entry.iterator?.return();signal.throwIfAborted();entry.iterator=entry.sink.canvases(at);entry.current=null;
    }
    entry.last=at;
    for(;;) {
      const value=entry.current;
      if(value&&value.timestamp<=at+1e-6&&at<value.timestamp+value.duration-1e-6) {
        if(Math.abs(value.timestamp-at)>1e-5)throw Error("Cached frame does not match its native CFR timestamp");
        const bitmap=await createImageBitmap(value.canvas);if(signal.aborted){bitmap.close();signal.throwIfAborted();}return bitmap;
      }
      const next=await entry.iterator.next();signal.throwIfAborted();
      if(next.done||!next.value||next.value.timestamp>at+1e-6)throw Error("Missing frame in preview segment");
      entry.current=next.value;
    }
  }
  async close(){this.generation++;await this.retain(new Set());}
}
