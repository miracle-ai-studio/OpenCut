import http from "node:http";
import { randomBytes } from "node:crypto";
import { checkAbort, reject } from "../src/validation.mjs";

// A single owned renderer may upload exactly its currently bound frame. This
// private loopback capability is never a project resource or a public API.
export async function openVideoRgbaTransport(realm, send, origin) {
  const secret = randomBytes(32).toString("hex");
  const sockets = new Set();
  const stats={};
  let active = false, reading=false, callbacks=0, consumed = null, closed = false;
  const server = http.createServer(async (request, response) => {
    const headers = { "Access-Control-Allow-Origin": origin, "Cache-Control": "no-store" };
    const encoded=!!realm.begin?.canvasEncoding&&realm.finalizing===true;
    const frame = encoded?realm.begin:realm.currentFrame;
    const current=()=>encoded?realm.finalizing&&realm.begin===frame:realm.currentFrame===frame;
    let owned = false,ownedRead=false;
    try {
      checkAbort(realm.signal);
      if(request.url===`/${secret}/callback`&&request.method==="POST") {
        if(closed||request.headers.origin!==origin||request.headers["content-type"]!=="text/plain"||callbacks>=8)reject("NOT_AUTHORIZED","Callback capability unavailable");
        callbacks++;
        try {
          const parts=[];let total=0;
          for await(const part of request){total+=part.length;if(total>1048576)reject("INVALID_HOST_CALLBACK","Callback exceeds private protocol admission");parts.push(part);}
          checkAbort(realm.signal);const packet=JSON.parse(Buffer.concat(parts,total).toString("utf8"));
          if(!packet||Object.keys(packet).sort().join()!=="args,op"||typeof packet.op!=="string")reject("INVALID_HOST_CALLBACK","Invalid exact callback packet");
          const result=await realm.videoCallback(packet.op,packet.args);checkAbort(realm.signal);
          response.writeHead(200,{...headers,"Content-Type":"application/json"});response.end(JSON.stringify(result));return;
        } finally {callbacks--;}
      }
      const resource=request.url?.match(new RegExp(`^/${secret}/resource/([0-9]+)/([A-Za-z0-9_-]{1,128})$`));
      if(resource&&request.method==="GET") {
        const descriptor=realm.videoTransfers?.get(resource[2]),currentFrame=realm.currentFrame;
        if(closed||reading||!currentFrame||resource[1]!==String(currentFrame.frameIndex)||descriptor?.frame!==currentFrame||request.headers.origin!==origin)
          reject("NOT_AUTHORIZED","Resource does not own the active frame");
        reading=true;ownedRead=true;
        response.writeHead(200,{...headers,"Content-Type":"application/octet-stream","Content-Length":descriptor.size});
        let offset=0;
        while(offset<descriptor.size){
          checkAbort(realm.signal);if(realm.currentFrame!==currentFrame)reject("RESOURCE_SOURCE_CLOSED","Resource frame retired");
          const count=Math.min(65536,descriptor.size-offset),chunk=await realm.videoReadResource(resource[2],offset,count);
          if(chunk.offset!==String(offset)||typeof chunk.bytesHex!=="string"||chunk.bytesHex.length!==count*2||!/^([a-f0-9]{2})+$/.test(chunk.bytesHex)||chunk.eof!==(offset+count===descriptor.size))reject("INVALID_VISUAL_PLAN","Resource transfer changed exact bytes");
          checkAbort(realm.signal);if(realm.currentFrame!==currentFrame)reject("RESOURCE_SOURCE_CLOSED","Resource frame retired");
          if(!response.write(Buffer.from(chunk.bytesHex,"hex")))await new Promise((resolve,fail)=>{
            const cleanup=()=>{response.off("drain",drain);response.off("close",close);};
            const drain=()=>{cleanup();resolve();},close=()=>{cleanup();fail(Object.assign(Error("Resource channel closed"),{code:"RESOURCE_SOURCE_CLOSED"}));};
            response.once("drain",drain);response.once("close",close);
          });
          offset+=count;
        }
        response.end();return;
      }
      if (closed || !frame || request.url !== `/${secret}/${encoded?"encoded":frame.frameIndex}` || request.headers.origin !== origin)
        reject("NOT_AUTHORIZED", "RGBA upload does not own the active frame");
      if (request.method === "OPTIONS") {
        response.writeHead(204, { ...headers, "Access-Control-Allow-Methods": "POST", "Access-Control-Allow-Headers": "content-type" });
        response.end(); return;
      }
      if (request.method !== "POST" || request.headers["content-type"] !== "application/octet-stream" || active || consumed === frame)
        reject("INVALID_OUTPUT_RGBA", "RGBA upload is not a single ordered frame");
      const expected = encoded?Number(request.headers["content-length"]):frame.plan.identity.width * frame.plan.identity.height * 4;
      if (!Number.isSafeInteger(expected) || expected < (encoded?1:4) || expected > Number(encoded?realm.begin.canvasEncoding.maxOutputBytes:realm.begin.admission.maxRgbaBytes))
        reject("INVALID_OUTPUT_RGBA", "RGBA frame exceeds captured admission");
      active = true; consumed = frame; owned = true;
      const began=performance.now();let packetMs=0;
      let offset = 0, pending = Buffer.alloc(0);
      const emit = async bytes => {
        checkAbort(realm.signal);
        if (!current()) reject("RESOURCE_SOURCE_CLOSED", "RGBA frame retired during upload");
        const packetStarted=performance.now();
        await send({ offsetBytes: String(offset), bytesHex: bytes.toString("hex") },encoded);
        packetMs+=performance.now()-packetStarted;
        checkAbort(realm.signal);
        if (!current()) reject("RESOURCE_SOURCE_CLOSED", "RGBA frame retired during upload");
        offset += bytes.length;
      };
      for await (const incoming of request) {
        if (incoming.length > expected - offset - pending.length)
          reject("INVALID_OUTPUT_RGBA", "RGBA upload exceeded its exact frame");
        // Node's HTTP reader is bounded; never collect a complete frame here.
        let bytes = pending.length ? Buffer.concat([pending, incoming]) : incoming;
        while (bytes.length >= 65536) { await emit(bytes.subarray(0, 65536)); bytes = bytes.subarray(65536); }
        pending = Buffer.from(bytes);
      }
      if (pending.length) { if (!encoded && pending.length % 4) reject("INVALID_OUTPUT_RGBA", "Partial RGBA pixel"); await emit(pending); }
      if (offset !== expected) reject("INVALID_OUTPUT_RGBA", "RGBA upload ended before the complete frame");
      stats.uploadMs=performance.now()-began;stats.packetMs=packetMs;
      response.writeHead(204, headers); response.end();
    } catch (error) {
      if(response.headersSent)response.destroy();
      else {response.writeHead(400, { ...headers, "Content-Type": "application/json" });response.end(JSON.stringify({ code: error.code ?? "INVALID_OUTPUT_RGBA" }));}
    } finally { if(owned)active = false;if(ownedRead)reading=false; }
  });
  server.on("connection", socket => { sockets.add(socket); socket.on("close", () => sockets.delete(socket)); });
  await new Promise((resolve, fail) => { server.once("error", fail); server.listen(0, "127.0.0.1", resolve); });
  const endpoint = `http://127.0.0.1:${server.address().port}/${secret}`;
  const close = async () => {
    if (closed) return; closed = true;
    for (const socket of sockets) socket.destroy();
    await new Promise(resolve => server.close(resolve));
  };
  realm.signal?.addEventListener("abort", () => { void close(); }, { once: true });
  return { endpoint, close, stats };
}
