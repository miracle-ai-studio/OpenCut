import { channel } from "node:diagnostics_channel";
import { ChildProcess } from "node:child_process";
import { AudioExecutionError, checkAbort, integer } from "../src/validation.mjs";

const exited = child => child.exitCode !== null || child.signalCode !== null;
function waitForExit(child) {
  if (exited(child)) return Promise.resolve();
  return new Promise((resolve, reject) => {
    const done = () => { clearTimeout(timer); child.off("exit", done); resolve(); };
    const timer = setTimeout(() => { child.off("exit", done); reject(new AudioExecutionError("AUDIO_REALM_TERMINATION_FAILED", "Owned browser exit was not acknowledged")); }, 1000);
    child.once("exit", done);
  });
}
function killGroup(child) {
  if (exited(child)) return;
  // Pinned Playwright launches Unix browsers in their own process group. This
  // handle comes from this launch's actual Node child, never a caller PID.
  try { process.kill(-child.pid, "SIGKILL"); } catch (error) { if (error.code !== "ESRCH") throw error; }
}
let launching = false;
export async function launchOwnedBrowser(chromium, options, signal) {
  checkAbort(signal);
  if (process.platform === "win32" || launching) throw new AudioExecutionError("CAPABILITY_UNAVAILABLE", "Owned browser launch requires one Unix host actor");
  const births = channel("child_process");
  let notified = false;
  const probe = () => { notified = true; };
  births.subscribe(probe);
  try { new ChildProcess(); } finally { births.unsubscribe(probe); }
  if (!notified) throw new AudioExecutionError("CAPABILITY_UNAVAILABLE", "Node child ownership observation is unavailable");
  launching = true;
  const pending = new Map(), roots = new Set(); let browser, killFailure;
  const cancelChild = child => { try { killGroup(child); } catch (error) { killFailure = error; } };
  const inspect = child => {
    if (child.spawnfile !== options.executablePath || !Number.isSafeInteger(child.pid) || child.pid < 1) return;
    roots.add(child);
    if (signal?.aborted) cancelChild(child);
  };
  const born = ({ process: child }) => {
    if (!(child instanceof ChildProcess)) return;
    const done = () => { inspect(child); child.off("spawn", done); child.off("error", done); pending.delete(child); };
    pending.set(child, done); child.once("spawn", done); child.once("error", done);
  };
  const abort = () => { for (const child of roots) cancelChild(child); };
  births.subscribe(born); signal?.addEventListener("abort", abort, { once: true });
  try {
    checkAbort(signal);
    browser = await chromium.launch(options);
    // A collocated launch may resolve in a microtask before Node's spawn event.
    for (const child of pending.keys()) inspect(child);
    checkAbort(signal);
    if (killFailure) throw killFailure;
    const alive = [...roots].filter(child => !exited(child));
    if (alive.length !== 1) throw new AudioExecutionError("AUDIO_REALM_OWNERSHIP_FAILED", "Browser launch did not yield one owned process");
    const child = alive[0]; integer(child.pid, "owned browser PID", 1);
    return {
      browser, process: () => child,
      async close() { await browser.close(); await waitForExit(child); },
      async kill() { killGroup(child); await waitForExit(child); },
    };
  } catch (error) {
    // Failed/aborted launch also owns any late children. Do not return before
    // their actual exit, or depend on a successful browser connection to kill.
    for (const child of pending.keys()) inspect(child);
    if (browser) {
      let timer;
      try { await Promise.race([browser.close(), new Promise((_, fail) => { timer=setTimeout(()=>fail(Error("Browser close timeout")),1000); })]); }
      catch { /* Owned group fallback below still requires actual exit. */ }
      finally { clearTimeout(timer); }
    }
    for (const child of roots) cancelChild(child);
    await Promise.all([...roots].map(waitForExit));
    checkAbort(signal); throw error;
  } finally {
    births.unsubscribe(born); signal?.removeEventListener("abort", abort);
    for (const [child, done] of pending) { child.off("spawn", done); child.off("error", done); }
    launching = false;
  }
}
