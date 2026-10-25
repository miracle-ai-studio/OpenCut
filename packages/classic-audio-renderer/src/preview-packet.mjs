import { AUDIO_EXECUTION_LIMITS, reject } from "./validation.mjs";

// Lossless private transport projection only. Rust supplies source clocks,
// gain and positions; the existing preview sampler consumes the expanded DTO.
const fail = () => reject("AUDIO_PROTOCOL_ERROR", "Invalid packed preview block");
const requireFact = condition => { if (!condition) fail(); };
const exactObject = (value, keys) => requireFact(value !== null && typeof value === "object" && !Array.isArray(value)
  && Object.keys(value).length === keys.length && keys.every(key => Object.hasOwn(value, key)));
const integer = (value, min, max) => requireFact(Number.isSafeInteger(value) && value >= min && value <= max);
const encoder = new TextEncoder();
// Rust str::trim uses Unicode White_Space, rather than JS trim's FEFF rule.
const whitespaceOnly = /^[\u0009-\u000d\u0020\u0085\u00a0\u1680\u2000-\u200a\u2028\u2029\u202f\u205f\u3000]*$/u;
const identifier = value => requireFact(typeof value === "string" && !whitespaceOnly.test(value)
  && encoder.encode(value).byteLength <= 1024 && !/[\u0000-\u001f\u007f-\u009f]/.test(value));
function decimal(value) {
  requireFact(typeof value === "string" && /^(0|[1-9][0-9]{0,19})$/.test(value));
  const number = BigInt(value);
  requireFact(number <= 18446744073709551615n);
  return number;
}
const decisions = new Set(["non_audio", "disabled_track", "disabled_element", "source_audio_disabled", "muted", "known_absent", "requires_probe", "ready", "outside_block"]);

function admitBlock(block, voiceFields, profile) {
  exactObject(block, ["identity", "startSample", "frameCount", "voices", "receipts"]);
  exactObject(block.identity, ["projectId", "revision", "sequenceId", "profile", "sampleRate", "channels"]);
  for (const key of ["projectId", "revision", "sequenceId"]) identifier(block.identity[key]);
  requireFact(block.identity.profile === profile && block.identity.channels === 2);
  if (profile === "classic-realtime-v31-web-audio") requireFact([44100, 48000].includes(block.identity.sampleRate));
  else integer(block.identity.sampleRate, 8000, 192000);
  const start = decimal(block.startSample);
  integer(block.frameCount, 1, AUDIO_EXECUTION_LIMITS.blockFrames);
  requireFact(start + BigInt(block.frameCount) <= 18446744073709551615n);
  requireFact(Array.isArray(block.voices) && block.voices.length <= AUDIO_EXECUTION_LIMITS.blockVoices && Array.isArray(block.receipts));
  const receiptIds = new Map();
  for (const [ordinal, receipt] of block.receipts.entries()) {
    exactObject(receipt, ["trackId", "elementId", "decision"]);
    identifier(receipt.trackId); identifier(receipt.elementId);
    requireFact(!receiptIds.has(receipt.elementId) && decisions.has(receipt.decision));
    receiptIds.set(receipt.elementId, ordinal);
  }
  const seen = new Set(); let work = 0, lastOrdinal = -1;
  // Complete collection admission precedes allocation of expanded positions.
  for (const voice of block.voices) {
    exactObject(voice, voiceFields);
    identifier(voice.elementId);
    requireFact(!seen.has(voice.elementId) && receiptIds.has(voice.elementId)); seen.add(voice.elementId);
    const ordinal = receiptIds.get(voice.elementId);
    requireFact(ordinal > lastOrdinal); lastOrdinal = ordinal;
    integer(voice.blockOffset, 0, block.frameCount);
    integer(voice.frameCount, 1, block.frameCount - voice.blockOffset);
    work += voice.frameCount;
    requireFact(work <= AUDIO_EXECUTION_LIMITS.voiceSamples);
  }
}

export function unpackPreviewBlock(packet) {
  exactObject(packet, ["block", "positionTables", "amplitudeTables"]);
  const { block, positionTables, amplitudeTables } = packet;
  admitBlock(block, ["elementId", "blockOffset", "frameCount", "positionRef", "amplitudeRef"], "classic-realtime-v31-web-audio");
  for (const tables of [positionTables, amplitudeTables]) {
    requireFact(Array.isArray(tables) && tables.length <= AUDIO_EXECUTION_LIMITS.blockVoices);
    let samples = 0;
    for (const table of tables) {
      requireFact(Array.isArray(table) && table.length > 0 && table.length <= AUDIO_EXECUTION_LIMITS.blockFrames);
      samples += table.length;
      requireFact(samples <= AUDIO_EXECUTION_LIMITS.voiceSamples);
    }
  }
  const positionsUsed = new Set(), amplitudesUsed = new Set();
  for (const voice of block.voices) {
    for (const [reference, tables, used] of [[voice.positionRef, positionTables, positionsUsed], [voice.amplitudeRef, amplitudeTables, amplitudesUsed]]) {
      integer(reference, 0, tables.length - 1);
      requireFact(tables[reference].length === voice.frameCount);
      if (!used.has(reference)) { requireFact(reference === used.size); used.add(reference); }
    }
  }
  requireFact(positionsUsed.size === positionTables.length && amplitudesUsed.size === amplitudeTables.length);
  // Each unique table is checked once. Execution admission above still counts
  // every voice frame, regardless of shared transport storage.
  for (const table of positionTables) for (const point of table) {
    if (point === null) continue;
    requireFact(Array.isArray(point) && point.length === 4);
    integer(point[0], 0, 0xffffffff);
    const lower = decimal(point[1]), upper = decimal(point[2]);
    requireFact(upper >= lower && upper - lower <= 1n && typeof point[3] === "number"
      && Number.isFinite(point[3]) && point[3] >= 0 && point[3] <= 1);
  }
  for (const table of amplitudeTables) for (const amplitude of table) requireFact(typeof amplitude === "number" && Number.isFinite(amplitude));
  const positions = positionTables.map(table => Object.freeze(table.map(point => point === null ? null : Object.freeze({
    segmentId: point[0], lower: point[1], upper: point[2], fraction: point[3],
  }))));
  for (const table of amplitudeTables) Object.freeze(table);
  return { ...block, voices: block.voices.map(({ positionRef, amplitudeRef, ...voice }) => ({ ...voice,
    positions: positions[positionRef], amplitudes: amplitudeTables[amplitudeRef],
  })) };
}

// Export uses exact integer source fractions. This expands transport tables
// only; the unchanged mixer still owns interpolation and Float32 write order.
export function unpackExportBlock(packet) {
  exactObject(packet, ["block", "positionTables", "amplitudeTables"]);
  const { block, positionTables, amplitudeTables } = packet;
  admitBlock(block, ["elementId", "blockOffset", "frameCount", "positionRef", "amplitudeRef"], "classic-export-v31-web-audio");
  for (const tables of [positionTables, amplitudeTables]) {
    requireFact(Array.isArray(tables) && tables.length <= AUDIO_EXECUTION_LIMITS.blockVoices);
    let samples = 0;
    for (const table of tables) {
      requireFact(Array.isArray(table) && table.length > 0 && table.length <= AUDIO_EXECUTION_LIMITS.blockFrames);
      samples += table.length; requireFact(samples <= AUDIO_EXECUTION_LIMITS.voiceSamples);
    }
  }
  const positionsUsed = new Set(), amplitudesUsed = new Set();
  for (const voice of block.voices) for (const [ref, tables, used] of [
    [voice.positionRef, positionTables, positionsUsed], [voice.amplitudeRef, amplitudeTables, amplitudesUsed],
  ]) {
    integer(ref, 0, tables.length - 1); requireFact(tables[ref].length === voice.frameCount);
    if (!used.has(ref)) { requireFact(ref === used.size); used.add(ref); }
  }
  requireFact(positionsUsed.size === positionTables.length && amplitudesUsed.size === amplitudeTables.length);
  const wide = value => {
    requireFact(typeof value === "string" && /^(0|[1-9][0-9]{0,38})$/.test(value));
    const n = BigInt(value); requireFact(n <= 340282366920938463463374607431768211455n); return n;
  };
  const positions = positionTables.map(table => Object.freeze(table.map(point => {
    requireFact(Array.isArray(point) && point.length === 3); decimal(point[0]);
    const numerator = wide(point[1]), denominator = wide(point[2]);
    requireFact(denominator > 0n && numerator < denominator);
    return Object.freeze({ lower: point[0], fractionNumerator: point[1], fractionDenominator: point[2] });
  })));
  for (const table of amplitudeTables) {
    for (const amplitude of table) requireFact(typeof amplitude === "number" && Number.isFinite(amplitude) && amplitude >= 0);
    Object.freeze(table);
  }
  return { ...block, voices: block.voices.map(({ positionRef, amplitudeRef, ...voice }) => ({ ...voice,
    positions: positions[positionRef], amplitudes: amplitudeTables[amplitudeRef],
  })) };
}

// Lossless private byte transport only. JSON facts and Float32 operation order
// remain unchanged; decompression cannot exceed the original 16 MiB admission.
export async function readPreviewFactsText(value){
 exactObject(value,["codec","decodedBytes","data"]);
 requireFact(value.codec==="deflate-base64"&&Number.isSafeInteger(value.decodedBytes)&&value.decodedBytes>0&&value.decodedBytes<=16777216);
 requireFact(typeof value.data==="string"&&value.data.length<=Math.ceil((16777216+65536)/3)*4&&value.data.length%4===0&&/^[A-Za-z0-9+/]*={0,2}$/.test(value.data));
 const binary=atob(value.data),bytes=new Uint8Array(binary.length);
 for(let i=0;i<binary.length;i++)bytes[i]=binary.charCodeAt(i);
 const reader=new Blob([bytes]).stream().pipeThrough(new DecompressionStream("deflate")).getReader(),decoder=new TextDecoder("utf-8",{fatal:true}),parts=[];
 let length=0;
 try{for(;;){const item=await reader.read();if(item.done)break;length+=item.value.length;requireFact(length<=value.decodedBytes);parts.push(decoder.decode(item.value,{stream:true}));}
  requireFact(length===value.decodedBytes);parts.push(decoder.decode());return parts.join("");
 }finally{await reader.cancel().catch(()=>{});reader.releaseLock();}
}
