import { requireFact, SourceArtError } from "./validation.mjs";
// Reject formats without a bounded header probe before asking the browser to
// allocate decoded pixels. Actual decoder dimensions must match this probe.
export function imageDimensions(bytes) {
  const v = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength), n = bytes.length;
  if (n >= 24 && v.getUint32(0) === 0x89504e47 && v.getUint32(4) === 0x0d0a1a0a && v.getUint32(12) === 0x49484452) return { width:v.getUint32(16), height:v.getUint32(20) };
  if (n >= 10 && String.fromCharCode(...bytes.subarray(0,6)).match(/^GIF8[79]a$/)) return { width:v.getUint16(6,true), height:v.getUint16(8,true) };
  if (n >= 26 && v.getUint16(0) === 0x424d && v.getUint32(14,true) >= 40) return {width:v.getInt32(18,true),height:Math.abs(v.getInt32(22,true))};
  if (n>=30 && v.getUint32(0)===0x52494646 && v.getUint32(8)===0x57454250) {
    if (v.getUint32(12)===0x56503858) return {width:1+bytes[24]+(bytes[25]<<8)+(bytes[26]<<16),height:1+bytes[27]+(bytes[28]<<8)+(bytes[29]<<16)};
    if (v.getUint32(12)===0x56503820 && v.getUint32(16,true)>=10 && bytes[23]===0x9d && bytes[24]===0x01 && bytes[25]===0x2a) return {width:v.getUint16(26,true)&0x3fff,height:v.getUint16(28,true)&0x3fff};
    if (v.getUint32(12)===0x5650384c && bytes[20]===0x2f) {const bits=v.getUint32(21,true);return{width:1+(bits&0x3fff),height:1+((bits>>>14)&0x3fff)};}
  }
  if (n >= 4 && v.getUint16(0) === 0xffd8) {
    let p=2;
    while(p+4<=n) {
      requireFact(bytes[p]===0xff); while(p<n && bytes[p]===0xff)p++;
      const marker=bytes[p++]; if(marker===0xda || marker===0xd9) break;
      if(marker===0x01 || marker>=0xd0&&marker<=0xd7)continue;
      if(p+2>n)break;const length=v.getUint16(p);requireFact(length>=2&&p+length<=n);
      if([0xc0,0xc1,0xc2,0xc3,0xc5,0xc6,0xc7,0xc9,0xca,0xcb,0xcd,0xce,0xcf].includes(marker)) {requireFact(length>=8);return{width:v.getUint16(p+5),height:v.getUint16(p+3)};}
      p+=length;
    }
  }
  throw new SourceArtError("UNSUPPORTED_SOURCE_ART_IMAGE", "Image has no supported bounded dimension header");
}
