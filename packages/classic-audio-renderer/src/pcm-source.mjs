import { integer, reject } from "./validation.mjs";

const windowChannels = new WeakMap(), windowSources = new WeakSet();
// Private PCM residency, not an authored timeline or a second source clock.
export function windowedPcmSource(sampleRate, length, channels, windows) {
  integer(length, "logical decoded frames", 1);
  integer(channels, "windowed channels", 1, 2);
  let end = 0;
  for (const window of windows) {
    integer(window.start, "retained sample start", end, length);
    integer(window.buffer.length, "retained sample count", 1, length - window.start);
    if (window.buffer.sampleRate !== sampleRate || window.buffer.numberOfChannels !== channels)
      reject("INVALID_DECODED_PCM", "Retained PCM format changed");
    end = window.start + window.buffer.length;
  }
  const views = Array.from({ length: channels }, (_, channel) => {
    const view = Object.freeze({ length });
    windowChannels.set(view, { last: 0, windows: windows.map(window => ({ start: window.start,
      data: window.buffer.getChannelData(channel) })) });
    return view;
  });
  const source = Object.freeze({ sampleRate, length, numberOfChannels: channels,
    residentFrames: windows.reduce((n, w) => n + w.buffer.length, 0),
    residentBytes: windows.reduce((n, w) => n + w.buffer.length * channels * 4, 0),
    getChannelData: channel => views[integer(channel, "windowed channel", 0, channels - 1)],
  });
  windowSources.add(source); return source;
}
export const isWindowedPcmSource = source => windowSources.has(source);
export function pcmValue(data, index) {
  const state = windowChannels.get(data);
  if (!state) return data[index];
  // Requested taps are native compiled global sample indices. Never substitute
  // silence for an unretained in-bounds sample or reset the interpolation phase.
  let found = state.windows[state.last];
  if (!found || index < found.start || index >= found.start + found.data.length) {
    let low = 0, high = state.windows.length;
    while (low < high) {
      const middle = (low + high) >>> 1;
      if (state.windows[middle].start <= index) low = middle + 1; else high = middle;
    }
    state.last = low - 1; found = state.windows[state.last];
  }
  if (found && (index < found.start || index >= found.start + found.data.length)) found = null;
  if (!found) reject("INVALID_AUDIO_PLAN", "Compiled sample is outside admitted PCM windows");
  return found.data[index - found.start];
}
export function copyPcmChannel(source, channel, start, count, destination) {
  const data = source.getChannelData(channel), state = windowChannels.get(data);
  if (!state) { destination.copyToChannel(data.subarray(start, start + count), channel); return; }
  const end = Math.min(data.length, start + count);
  let offset = start;
  for (const window of state.windows) {
    const from = Math.max(offset, window.start), to = Math.min(end, window.start + window.data.length);
    if (to <= from) continue;
    if (from !== offset) reject("INVALID_AUDIO_PLAN", "Prepared source slice has a missing PCM window");
    destination.copyToChannel(window.data.subarray(from - window.start, to - window.start), channel, from - start);
    offset = to;
  }
  if (offset < end) reject("INVALID_AUDIO_PLAN", "Prepared source slice exceeds retained PCM");
}
