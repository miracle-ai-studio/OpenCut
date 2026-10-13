// Independent authored schedule: 20 unit-rate eight-second voices across300s.
// Interior samples avoid resampler/clip-edge rounding; no production IR is read.
export function densePcmWindows() {
  return Array.from({length: 20}, (_, placement) => {
    const start = Math.ceil(292 * placement * 48000 / 19);
    const source = {id: `${placement}:source`, placement, kind: "source", start_sample: start + 60000, frames: 4096};
    return placement < 19 ? [source, {id: `${placement}:gap`, placement, kind: "gap", start_sample: start + 432000, frames: 4096}] : [source];
  }).flat();
}
// The ended response describes the terminal control cursor; it need not retain
// the stream's original start. Observe that zero origin before accepting bytes.
export function observePcmOrigin(observed, session) {
  if (session.identity.sample_rate !== 48000) throw Error("Unexpected PCM sample rate");
  if (observed) return true;
  if (session.state === "ended" || session.state === "stopped") throw Error("No original PCM origin observation");
  if (session.actual_start_sample !== "0") throw Error("PCM did not begin at authored zero origin");
  return true;
}
export function measurePcmWindow(window) {
  if (window.values.length !== window.frames * 2 || !window.values.every(Number.isFinite)) throw Error("Incomplete/nonfinite stereoPCM");
  let difference = 0;
  const channels = [0, 1].map(channel => {
    let square = 0, peak = 0;
    const crossings = [];
    for (let i = 0; i < window.frames; i++) {
      const value = window.values[i * 2 + channel];
      square += value * value; peak = Math.max(peak, Math.abs(value));
      if (i > 0) {
        const before = window.values[(i - 1) * 2 + channel];
        if (before <= 0 && value > 0) crossings.push(i - 1 - before / (value - before));
      }
      difference = Math.max(difference, Math.abs(window.values[i * 2] - window.values[i * 2 + 1]));
    }
    return {rms: Math.sqrt(square / window.frames), peak,
      frequency_hz: crossings.length >= 2 ? (crossings.length - 1) * 48000 / (crossings.at(-1) - crossings[0]) : null};
  });
  const expectedRms = .125 / Math.sqrt(2) * 3 * 10 ** (-18 / 20);
  if (difference > 1e-7) throw Error(`Stereo mismatch: ${window.id}`);
  for (const channel of channels) {
    if (window.kind === "gap") { if (channel.peak !== 0) throw Error(`Gap is not exact silence: ${window.id}`); }
    else if (window.kind !== "source" || channel.frequency_hz === null ||
        Math.abs(channel.frequency_hz - 440) >= 1 || Math.abs(channel.rms - expectedRms) >= .0005)
      throw Error(`PCM differs from authored three440Hz voices at-18dB: ${window.id}`);
  }
  const {values, ...identity} = window;
  return {...identity, channels, stereo_max_difference: difference};
}
