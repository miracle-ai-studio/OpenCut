import test from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { channel } from "node:diagnostics_channel";
import { launchOwnedBrowser } from "../host/owned-browser.mjs";

const options = { executablePath: process.execPath };
const child = () => spawn(process.execPath, ["-e", "setInterval(()=>{},1000)"], { detached: true, stdio: "ignore" });
const finished = c => c.exitCode !== null || c.signalCode !== null;
test("collocated launch preserves actual process handle; close waits for real exit and unsubscribes", async () => {
  let c;
  const owned = await launchOwnedBrowser({ async launch() { c=child(); return { async close() { c.kill("SIGTERM"); } }; } }, options);
  assert.equal(owned.process(), c); assert(c.pid>0); assert(!finished(c));
  await owned.close(); assert(finished(c)); assert.equal(channel("child_process").hasSubscribers, false);
});
test("owned group kill acknowledges real exit even when browser close cannot respond", async () => {
  let c;
  const owned = await launchOwnedBrowser({ async launch() { c=child(); return { close:()=>new Promise(()=>{}) }; } }, options);
  await owned.kill(); assert.equal(c.signalCode, "SIGKILL");
  await owned.kill(); assert(finished(c));
});
test("abort during launch kills a late owned child and does not publish a usable owner", async () => {
  const controller=new AbortController(); let c;
  const launching=launchOwnedBrowser({ async launch() {
    await new Promise(resolve=>setTimeout(resolve,20));c=child();
    await once(c,"spawn"); await new Promise(resolve=>setTimeout(resolve,20));
    return { async close() { if(!finished(c))c.kill("SIGTERM"); } };
  } }, options, controller.signal);
  controller.abort();await assert.rejects(launching,{code:"CANCELLED"});assert(finished(c));
  assert.equal(channel("child_process").hasSubscribers,false);
});
test("launch rejection cleans its captured process and preserves the original error", async () => {
  let c;const failure=Error("launch failed");
  await assert.rejects(launchOwnedBrowser({ async launch(){c=child();await once(c,"spawn");throw failure;} },options),error=>error===failure);
  assert(finished(c));assert.equal(channel("child_process").hasSubscribers,false);
});
test("abort before launch starts no child; unrelated executable cannot be claimed", async () => {
  const controller=new AbortController();controller.abort();let calls=0;
  await assert.rejects(launchOwnedBrowser({ async launch(){calls++;} },options,controller.signal),{code:"CANCELLED"});assert.equal(calls,0);
  let c,closed=false;
  await assert.rejects(launchOwnedBrowser({ async launch(){c=child();return {async close(){closed=true;c.kill("SIGTERM");await once(c,"exit");}};} },{executablePath:"/unrelated"}),{code:"AUDIO_REALM_OWNERSHIP_FAILED"});
  assert(closed);assert(finished(c));
});
test("ambiguous captured roots fail closed and both actual children exit", async () => {
  let a,b;
  await assert.rejects(launchOwnedBrowser({ async launch(){
    a=child();b=child();await Promise.all([once(a,"spawn"),once(b,"spawn")]);
    return {async close(){}};
  } },options),{code:"AUDIO_REALM_OWNERSHIP_FAILED"});
  assert(finished(a));assert(finished(b));assert.equal(channel("child_process").hasSubscribers,false);
});
test("one host actor rejects a concurrent launch without spawning or stealing the first owner", async () => {
  let release,c,calls=0;
  const first=launchOwnedBrowser({async launch(){
    await new Promise(resolve=>{release=resolve;});c=child();return {async close(){c.kill("SIGTERM");}};
  }},options);
  try {
    await assert.rejects(launchOwnedBrowser({async launch(){calls++;}},options),{code:"CAPABILITY_UNAVAILABLE"});
    assert.equal(calls,0);
  } finally {release();const owner=await first;await owner.close();}
  assert(finished(c));assert.equal(channel("child_process").hasSubscribers,false);
});
