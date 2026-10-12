import test from "node:test";
import assert from "node:assert/strict";
import { summarizePreviewConsumption } from "./scene-preview-consumption.mjs";
const point = (renderFrame, copied, frames, at, completedBlocks = 0) => ({ renderFrame, quantumFrames: 128, copied, frames: String(frames), at, completedBlocks, contextFrame: renderFrame+128 });
test("batched arrival cannot hide independently known silent device quanta or create consumed samples", () => {
  const reports = [point(1000,128,128,0),point(1128,0,128,10),point(1256,0,128,10.01),point(1384,128,256,10.02)];
  const r = summarizePreviewConsumption(reports,[],[],48000);
  assert.equal(r.status,"complete_observed_span");
  assert.equal(r.rendered_frames,384); assert.equal(r.copied_frames,128); assert.equal(r.missing_frames,256);
  assert.equal(r.render_quantum_deficit_ratio,2/3);
  assert.equal(r.quantum_delivery_lag.maximum_ms,0);
});
test("partial quantum copies and large exact cursors retain the original end-to-end sample delta", () => {
  const initial=9007199254740993n;
  const reports=[point(0,128,initial,0),point(128,64,initial+64n,3),point(256,128,initial+192n,6)];
  reports[1].contextFrame=512;
  const r=summarizePreviewConsumption(reports,[],[],48000);
  assert.equal(r.render_quantum_deficit_ratio,.25); assert.equal(r.missing_frames,64);
  assert.equal(r.quantum_delivery_lag.maximum_ms,256/48);
});
test("missing, duplicated, reordered, invalid and dishonest cursor receipts do not establish a complete span", () => {
  const first=point(0,128,128,0), second=point(128,128,256,3);
  for (const change of [{renderFrame:256},{renderFrame:0},{frames:"257"},{completedBlocks:-1},{copied:129},{frames:"01"},{quantumFrames:0},{renderFrame:undefined},{at:NaN},{at:-1}]) {
    assert.notEqual(summarizePreviewConsumption([first,{...second,...change}],[],[],48000).status,"complete_observed_span");
  }
  assert.equal(summarizePreviewConsumption([first],[],[],48000).status,"unavailable");
});
test("original two-slot completion credit matches only the next refill block and a covering consumer report", () => {
  const reports=[point(0,128,3968,0),point(128,128,4096,10,1),point(256,128,4224,11,1)];
  const controls=[{kind:"report_consumed",frames:"3968",began:11},{kind:"heartbeat",frames:"4096",began:12},{kind:"report_consumed",frames:"4224",began:15}];
  const supplies=[{ordinal:2,at:10.5},{ordinal:3,at:40}];
  const r=summarizePreviewConsumption(reports,controls,supplies,48000);
  assert.equal(r.completion_to_report.maximum_ms,5);
  assert.equal(r.completion_to_next_supply.maximum_ms,30);
  assert.equal(r.report_to_next_supply.maximum_ms,25);
});
