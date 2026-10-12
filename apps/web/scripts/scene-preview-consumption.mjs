// Verifier-only arithmetic over observed device receipts. No source clock,
// project IR, production credit or acceptance threshold is computed here.
const decimal = value => typeof value === "string" && /^(0|[1-9][0-9]*)$/.test(value) && value.length <= 20 && BigInt(value) <= 18446744073709551615n;
const whole = value => Number.isSafeInteger(value) && value >= 0;
const timings = values => values.length ? { count: values.length, mean_ms: values.reduce((a,b)=>a+b,0)/values.length,
  maximum_ms: Math.max(...values), p95_ms: [...values].sort((a,b)=>a-b)[Math.ceil(values.length*.95)-1] } : { count: 0 };

export function summarizePreviewConsumption(reports, controls, supplies, sampleRate) {
  if (![44100,48000].includes(sampleRate) || !Array.isArray(reports) || reports.length < 2 || reports.length > 8192)
    return { status: "unavailable", reason: "Missing bounded device quantum observations" };
  if (reports.some(p => !whole(p.renderFrame) || !whole(p.quantumFrames) || p.quantumFrames < 1 || p.quantumFrames > 4096 ||
      !whole(p.copied) || p.copied > p.quantumFrames || !whole(p.contextFrame) || !whole(p.completedBlocks) || !decimal(p.frames) ||
      !Number.isSafeInteger(p.renderFrame+p.quantumFrames) || !Number.isFinite(p.at) || p.at < 0))
    return { status: "unavailable", reason: "Invalid or missing device quantum facts" };
  let copied = 0, rendered = 0;
  for (let i = 1; i < reports.length; i++) {
    const previous = reports[i-1], current = reports[i];
    if (current.renderFrame !== previous.renderFrame + previous.quantumFrames || current.at < previous.at || current.completedBlocks < previous.completedBlocks ||
        BigInt(current.frames) - BigInt(previous.frames) !== BigInt(current.copied))
      return { status: "incomplete", reason: "Missing, reordered or inconsistent render quantum receipts" };
    copied += current.copied; rendered += current.quantumFrames;
  }
  const completions = reports.slice(1).filter((p,i) => p.completedBlocks > reports[i].completedBlocks);
  const release = [], refill = [], turnaround = [];
  for (const p of completions) {
    const control = controls.find(c => c.kind === "report_consumed" && decimal(c.frames) && BigInt(c.frames) >= BigInt(p.frames) && c.began >= p.at);
    const supply = supplies.find(s => s.ordinal === p.completedBlocks + 2 && s.at >= p.at);
    if (!control || !supply) continue;
    release.push(control.began - p.at); refill.push(supply.at - p.at);
    if (supply.at >= control.began) turnaround.push(supply.at - control.began);
  }
  return { status: "complete_observed_span", sample_rate: sampleRate,
    rendered_frames: rendered, copied_frames: copied, missing_frames: rendered - copied,
    render_quantum_deficit_ratio: (rendered - copied) / rendered,
    quantum_delivery_lag: timings(reports.map(p => Math.max(0,p.contextFrame-p.renderFrame-p.quantumFrames)*1000/sampleRate)),
    context_before_quantum_end_count: reports.filter(p => p.contextFrame < p.renderFrame+p.quantumFrames).length,
    completion_to_report: timings(release), completion_to_next_supply: timings(refill), report_to_next_supply: timings(turnaround),
    completion_receipts_observed: completions.length,
    method: "Contiguous device renderFrame/quantumFrames and exact copied/cursor deltas. Context lag is page-observed AudioContext graph-frame lag, not DAC latency. Completion k is correlated with transferred block k+2 under the existing two-slot FIFO; HTTP timing includes native production and page scheduling, not a causal native breakdown." };
}
