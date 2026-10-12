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

function admitBlock(block, voiceFields) {
  exactObject(block, ["identity", "startSample", "frameCount", "voices", "receipts"]);
  exactObject(block.identity, ["projectId", "revision", "sequenceId", "profile", "sampleRate", "channels"]);
  for (const key of ["projectId", "revision", "sequenceId"]) identifier(block.identity[key]);
  requireFact(block.identity.profile === "classic-realtime-v31-web-audio"
    && [44100, 48000].includes(block.identity.sampleRate) && block.identity.channels === 2);
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

export function unpackPreviewBlockV1(packet) {
  exactObject(packet, ["schemaVersion", "block"]);
  requireFact(packet.schemaVersion === 1);
  const block = packet.block;
  admitBlock(block, ["elementId", "blockOffset", "frameCount", "positions", "amplitudes"]);
  for (const voice of block.voices) {
    requireFact(Array.isArray(voice.positions)
      && voice.positions.length === voice.frameCount && Array.isArray(voice.amplitudes) && voice.amplitudes.length === voice.frameCount);
    for (const amplitude of voice.amplitudes) requireFact(typeof amplitude === "number" && Number.isFinite(amplitude));
    for (const point of voice.positions) {
      if (point === null) continue;
      requireFact(Array.isArray(point) && point.length === 4);
      integer(point[0], 0, 0xffffffff);
      const lower = decimal(point[1]), upper = decimal(point[2]);
      requireFact(upper >= lower && upper - lower <= 1n && typeof point[3] === "number"
        && Number.isFinite(point[3]) && point[3] >= 0 && point[3] <= 1);
    }
  }
  return { ...block, voices: block.voices.map(voice => ({ ...voice,
    positions: voice.positions.map(point => point === null ? null : {
      segmentId: point[0], lower: point[1], upper: point[2], fraction: point[3],
    }),
  })) };
}

export function unpackPreviewBlockV2(packet) {
  exactObject(packet, ["schemaVersion", "block", "positionTables", "amplitudeTables"]);
  requireFact(packet.schemaVersion === 2);
  const { block, positionTables, amplitudeTables } = packet;
  admitBlock(block, ["elementId", "blockOffset", "frameCount", "positionRef", "amplitudeRef"]);
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
