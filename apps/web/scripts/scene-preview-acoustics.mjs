// Authored signal oracles: no renderer PCM, DSP implementation or product
// constants are imported. Phases are fitted independently at each observation
// point because the actual compressor has lookahead; this is not a phase oracle.
const FREQUENCIES = [440, 880, 660];
export const chromiumLowMasterGain = 0.98 * 10 ** (0.95 * 0.6 / 20);
// Classic threshold -1dB, knee0, ratio20, headroom.98. Chromium's steady
// compressor uses maximum-channel detection and perceptual makeup gain:
// https://raw.githubusercontent.com/chromium/chromium/main/third_party/blink/renderer/platform/audio/dynamics_compressor.cc
// Process (makeup/max channel) and Saturate (zero-knee static curve). This
// independently derived expectation is scoped to the recorded Chrome runtime,
// not a claim that all WebAudio implementations have the same PCM.
export const chromiumHotMasterLeft = chromiumLowMasterGain *
  10 ** ((-1 + (20 * Math.log10(2) + 1) / 20) / 20);

export function acousticCaptureMatches(session, progress, record, {db, hot}) {
  if (!session || session.state !== "playing" || session.identity.sample_rate !== 48000 ||
      (session.temporary_gain?.decibels_tenths ?? 0) !== db * 10 ||
      !progress?.token || BigInt(progress.consumedFrames) <= 0n ||
      !record?.window || record.window.token !== progress.token ||
      record.session?.session_id !== session.session_id ||
      record.session.generation !== session.generation || record.session.overlay_epoch !== session.overlay_epoch)
    return false;
  const start = Number(session.actual_start_sample) / 48000;
  return Number.isFinite(start) && start >= (hot ? 6 : 0) && start + .75 + 4096 / 48000 < (hot ? 8 : 6);
}

export function measureTaggedStereo(values) {
  if (values.length !== 8192 || !values.every(Number.isFinite))
    throw Error("Expected4096 finite interleaved stereo frames");
  return [0, 1].map(channel => {
    const normal = Array.from({length: 7}, () => new Array(8).fill(0));
    for (let i = 0; i < 4096; i++) {
      const row = [1, ...FREQUENCIES.flatMap(hz => {
        const phase = 2 * Math.PI * hz * i / 48000;
        return [Math.sin(phase), Math.cos(phase)];
      })];
      for (let j = 0; j < 7; j++) {
        for (let k = 0; k < 7; k++) normal[j][k] += row[j] * row[k];
        normal[j][7] += row[j] * values[2 * i + channel];
      }
    }
    for (let j = 0; j < 7; j++) {
      let pivot = j;
      for (let k = j + 1; k < 7; k++)
        if (Math.abs(normal[k][j]) > Math.abs(normal[pivot][j])) pivot = k;
      [normal[j], normal[pivot]] = [normal[pivot], normal[j]];
      const scale = normal[j][j];
      if (Math.abs(scale) < 1e-8) throw Error("Singular acoustic basis");
      for (let k = j; k < 8; k++) normal[j][k] /= scale;
      for (let n = 0; n < 7; n++) if (n !== j) {
        const factor = normal[n][j];
        for (let k = j; k < 8; k++) normal[n][k] -= factor * normal[j][k];
      }
    }
    const coefficients = normal.map(row => row[7]);
    let residual = 0, peak = 0;
    for (let i = 0; i < 4096; i++) {
      let expected = coefficients[0];
      FREQUENCIES.forEach((hz, k) => {
        const phase = 2 * Math.PI * hz * i / 48000;
        expected += coefficients[1 + 2 * k] * Math.sin(phase) + coefficients[2 + 2 * k] * Math.cos(phase);
      });
      const value = values[2 * i + channel];
      residual += (value - expected) ** 2;
      peak = Math.max(peak, Math.abs(value));
    }
    return {dc: coefficients[0], peak, residual_rms: Math.sqrt(residual / 4096),
      amplitudes: FREQUENCIES.map((_, k) => Math.hypot(coefficients[1 + 2 * k], coefficients[2 + 2 * k]))};
  });
}
export function assertTaggedStereo(values, {db = 0, postMaster = false, hot = false} = {}) {
  const channels = measureTaggedStereo(values);
  const gain = postMaster ? chromiumLowMasterGain : 1;
  const target = 0.125 * 10 ** (db / 20);
  const expected = hot ? [[], []] : [[target, .1, 0], [target, 0, .05]];
  channels.forEach((channel, c) => {
    if (channel.residual_rms > .0001) throw Error("Unexpected acoustic residual");
    const dc = hot ? (postMaster ? chromiumHotMasterLeft : 2) / (c + 1) : 0;
    if (Math.abs(channel.dc - dc) > .0005) throw Error("Incorrect DC/mastering/channel linkage");
    for (let k = 0; k < 3; k++) {
      const amplitude = hot ? 0 : expected[c][k] * gain;
      if (Math.abs(channel.amplitudes[k] - amplitude) > .0001)
        throw Error(`Incorrect voice/channel amplitude: channel${c}, frequency${FREQUENCIES[k]}`);
    }
  });
  return channels;
}

// Acceptance-only sink. The product's original Gain→destination connection
// stays intact. This second branch emits exact zero and reports one bounded
// interior window per actual context/token, never PCM back to the product.
export function masteringWitnessProcessor() {
  class Witness extends AudioWorkletProcessor {
    constructor() {
      super();
      this.token = null; this.first = null; this.frames = 0;
      this.values = new Float32Array(8192);
      this.port.onmessage = ({data}) => {
        if (data.kind !== "arm" || typeof data.token !== "string" || this.token)
          throw Error("Invalid mastering witness ownership");
        this.token = data.token;
      };
    }
    process(inputs, outputs) {
      for (const output of outputs) for (const channel of output) channel.fill(0);
      if (this.done) return false;
      const input = inputs[0];
      if (!this.token || input.length !== 2) return true;
      for (let i = 0; i < input[0].length; i++) {
        if (this.first === null && (input[0][i] !== 0 || input[1][i] !== 0))
          this.first = currentFrame + i;
        if (this.first === null || currentFrame + i < this.first + 36000) continue;
        this.values[this.frames * 2] = input[0][i];
        this.values[this.frames * 2 + 1] = input[1][i];
        if (++this.frames === 4096) {
          this.done = true;
          this.port.postMessage({kind: "window", token: this.token, first_nonzero_frame: this.first,
            start_context_frame: this.first + 36000, frames: this.frames, values: this.values.buffer}, [this.values.buffer]);
          return false;
        }
      }
      return true;
    }
  }
  registerProcessor("opencut-mastering-acceptance", Witness);
}
export function installMasteringWitness({processorSource}) {
  const records = [], contexts = new WeakMap();
  window.__sceneMasteringWitness = {records, overflows: 0};
  const NativeWorker = window.Worker;
  window.Worker = class extends NativeWorker {
    constructor(...args) {
      super(...args);
      if (args[1]?.name !== "opencut-audio-pump") return;
      this.addEventListener("message", ({data}) => {
        if (data.channel !== "opencut-audio-pump" || data.version !== 1 || data.kind !== "progress") return;
        const record = records.find(r => r.token === data.token);
        if (!record) throw Error("Missing mastering quantum owner");
        const q = data.receipt;
        if (record.window && q.renderFrame >= record.window.start_context_frame + record.window.frames) return;
        if (record.quanta.length === 1024) { record.quantum_overflows++; return; }
        record.quanta.push({token:q.token,copied:q.copied,consumedFrames:q.consumedFrames,
          completedBlocks:q.completedBlocks,renderFrame:q.renderFrame,quantumFrames:q.quantumFrames});
      });
      const post = this.postMessage.bind(this);
      this.postMessage = (data, ...rest) => {
        if (data.channel === "opencut-audio-pump" && data.version === 1 && data.kind === "start") {
          const record = records.find(r => r.token === data.token);
          if (!record || record.session) throw Error("Missing/duplicate mastering Worker ownership");
          const s = data.session;
          record.session = {session_id: s.session_id, generation: s.generation,
            overlay_epoch: s.overlay_epoch, identity: structuredClone(s.identity)};
        }
        return post(data, ...rest);
      };
    }
  };
  const Native = window.AudioWorkletNode;
  const add = AudioWorklet.prototype.addModule;
  AudioWorklet.prototype.addModule = async function(...args) {
    await add.call(this, ...args);
    const url = URL.createObjectURL(new Blob([processorSource], {type: "text/javascript"}));
    try { await add.call(this, url); } finally { URL.revokeObjectURL(url); }
  };
  const connect = AudioNode.prototype.connect;
  AudioNode.prototype.connect = function(...args) {
    const result = connect.call(this, ...args);
    if (!(this instanceof GainNode) || args[0] !== this.context.destination) return result;
    if (contexts.has(this.context)) throw Error("Ambiguous output mastering witness");
    if (records.length === 8) {
      window.__sceneMasteringWitness.overflows++;
      throw Error("Mastering observation exceeds8 contexts");
    }
    const record = {token: null, sample_rate: this.context.sampleRate,
      output_gain: this.gain.value, window: null, quanta: [], quantum_overflows: 0};
    const node = new Native(this.context, "opencut-mastering-acceptance", {
      numberOfInputs: 1, numberOfOutputs: 1, outputChannelCount: [2],
      channelCount: 2, channelCountMode: "explicit", channelInterpretation: "discrete",
    });
    node.port.onmessage = ({data}) => {
      if (data.kind !== "window" || data.token !== record.token || record.window || data.values.byteLength !== 32768)
        throw Error("Invalid mastering observation receipt");
      record.window = {...data, values: Array.from(new Float32Array(data.values))};
    };
    records.push(record); contexts.set(this.context, {record, node});
    connect.call(this, node); connect.call(node, this.context.destination);
    return result;
  };
  window.AudioWorkletNode = class extends Native {
    constructor(...args) {
      super(...args);
      if (args[1] !== "opencut-v4-audio-preview") return;
      const post = this.port.postMessage.bind(this.port);
      this.port.postMessage = (data, ...rest) => {
        if (data.kind === "reset") {
          const entry = contexts.get(args[0]);
          if (!entry || entry.record.token) throw Error("Missing/duplicate mastering observation token");
          entry.record.token = data.token;
          entry.node.port.postMessage({kind: "arm", token: data.token});
        }
        return post(data, ...rest);
      };
    }
  };
}
