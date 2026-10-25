// Reuse Classic's CanvasSource/Mediabunny encoder after the native scene binding.
// No project sampling or geometry is evaluated in this adapter.
import {Output,CanvasSource,Mp4OutputFormat,StreamTarget} from "mediabunny";
import {integer,reject,sameJsonFacts} from "./validation.mjs";
export async function createCanvasVideo(identity,encoding) {
  const width=integer(identity.width,"canvas width",1,16384),height=integer(identity.height,"canvas height",1,16384);
  const numerator=integer(encoding.fpsNumerator,"CFR numerator",1),denominator=integer(encoding.fpsDenominator,"CFR denominator",1);
  const maximum=integer(encoding.maxOutputBytes,"encoded preview ceiling",1,8*1024*1024);
  const bitrate=integer(encoding.bitrate,"Classic video bitrate",1,2**32-1);
  const fixed=Object.fromEntries(["projectId","revision","sequenceId","width","height","profile"].map(key=>[key,identity[key]]));
  const canvas=new OffscreenCanvas(width,height),chunks=[];let allocated=0,bytes=0,next=0,closed=false,reserve;
  const ensure=end=>{while(allocated<end){const size=Math.min(65536,maximum-allocated);reserve?.(size);chunks.push(new Uint8Array(size));allocated+=size;}};
  const target=new StreamTarget(new WritableStream({write(chunk){
    if(chunk.type!=="write"||!Number.isSafeInteger(chunk.position)||chunk.position<0||!(chunk.data instanceof Uint8Array)||chunk.data.length>maximum-chunk.position)
      reject("VIDEO_BUDGET_EXCEEDED","Encoded preview exceeds ordered output admission");
    ensure(chunk.position+chunk.data.length);
    let at=chunk.position,offset=0;
    while(offset<chunk.data.length){const part=chunks[Math.floor(at/65536)],inside=at%65536,count=Math.min(part.length-inside,chunk.data.length-offset);part.set(chunk.data.subarray(offset,offset+count),inside);offset+=count;at+=count;}
    bytes=Math.max(bytes,chunk.position+chunk.data.length);
  }}),{chunked:true,chunkSize:65536});
  const output=new Output({format:new Mp4OutputFormat({fastStart:"fragmented"}),target});
  const source=new CanvasSource(canvas,{codec:"avc",bitrate,latencyMode:"quality",hardwareAcceleration:"prefer-hardware"});
  output.addVideoTrack(source,{frameRate:numerator/denominator});await output.start();
  return {canvas,get retainedBytes(){return bytes;},maximum,
    account(reserveBytes){
      if(reserve)reject("INVALID_HOST_PROTOCOL","Concurrent canvas memory admission");
      const releases=[reserveBytes(allocated+65536+width*height*4*4)];
      reserve=size=>releases.push(reserveBytes(size));
      return ()=>{reserve=undefined;for(const release of releases)release();};
    },
    async add(current,index){
      if(closed||index!==next||!sameJsonFacts(fixed,Object.fromEntries(Object.keys(fixed).map(key=>[key,current[key]]))))
        reject("INVALID_HOST_PROTOCOL","Canvas encoder changed its fixed frame sequence");
      await source.add(index*denominator/numerator,denominator/numerator);next++;
    },
    async finish(count){
      if(closed||count!==next)reject("INVALID_HOST_PROTOCOL","Incomplete canvas encoder sequence");closed=true;
      source.close();await output.finalize();
      if(!bytes||bytes>maximum)reject("VIDEO_BUDGET_EXCEEDED","Invalid final encoded preview size");
      return new Blob(chunks.map((chunk,index)=>chunk.subarray(0,Math.min(chunk.length,bytes-index*65536))),{type:"video/mp4"});
    },
    async cancel(){if(!closed){closed=true;await output.cancel();}canvas.width=0;canvas.height=0;for(const chunk of chunks)chunk.fill(0);chunks.length=0;}
  };
}
