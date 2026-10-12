import { AudioExecutionError, checkAbort, reject } from "./validation.mjs";

// One termination acknowledgement covers the complete execution, including
// hashing, synchronous block boundaries and context cleanup. Child awaits use
// the same memoized port, so nested listeners cannot terminate twice.
export function executionCancellation(host, signal) {
  let termination;
  const terminate = () => {
    if (!termination) {
      termination = Promise.resolve().then(() => host.abortRendering()).catch(() => {
        throw new AudioExecutionError("AUDIO_CANCELLATION_FAILED", "The host failed to terminate its audio execution realm");
      });
      // Abort events cannot await this promise. The outer execution fence owns
      // propagation; suppress an unhandled rejection before it reaches catch.
      termination.catch(() => {});
    }
    return termination;
  };
  const listener = () => { void terminate(); };
  signal?.addEventListener("abort", listener, { once: true });
  return {
    host: new Proxy(host, { get(target, name) {
      if (name === "abortRendering") return terminate;
      const value = Reflect.get(target, name, target);
      return typeof value === "function" ? value.bind(target) : value;
    } }),
    terminate,
    dispose() { signal?.removeEventListener("abort", listener); },
  };
}

// The host must terminate the isolated browser realm/process on cancellation.
// OfflineAudioContext has no close() API: rejecting a promise alone is not a
// cancellation implementation. Await the host's termination acknowledgement.
export async function cancellable(work, host, signal) {
  if (!signal) return work;
  let listener, requested = false;
  const cancelled = new Promise((_, fail) => {
    listener = () => {
      if (requested) return;
      requested = true;
      Promise.resolve().then(() => host.abortRendering()).then(
        () => { try { checkAbort(signal); } catch (error) { fail(error); } },
        () => fail(new AudioExecutionError("AUDIO_CANCELLATION_FAILED", "The host failed to terminate its audio execution realm")),
      );
    };
    signal.addEventListener("abort", listener, { once: true });
    if (signal.aborted) listener();
  });
  try {
    const result = await Promise.race([work, cancelled]);
    // If completion and abort arrive together, acknowledge termination before
    // propagating CANCELLED rather than letting a successful promise win.
    if (signal.aborted) return await cancelled;
    return result;
  } catch (error) {
    // A simultaneous ordinary failure also crosses the termination fence.
    if (signal.aborted) return await cancelled;
    throw error;
  }
  finally { signal.removeEventListener("abort", listener); }
}
export function requireHost(host) {
  for (const name of ["resolveVerifiedBytes", "admitDecode", "compilePitchPreparation", "compileSourcePositions", "compileBlock", "createAudioContext", "createOfflineContext", "abortRendering"]) {
    if (typeof host[name] !== "function") reject("MISSING_AUDIO_HOST_PORT", `Missing ${name} host port`);
  }
  if (typeof host.engineIdentity !== "string" || !host.engineIdentity.trim()) reject("MISSING_AUDIO_HOST_PORT", "An actual browser execution identity is required");
}
