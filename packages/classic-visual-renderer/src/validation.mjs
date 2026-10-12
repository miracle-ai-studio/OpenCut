export class VisualExecutionError extends Error {
  constructor(code, message) { super(message); this.code = code; }
}
export function reject(code, message) { throw new VisualExecutionError(code, message); }
export function integer(value, name, minimum = 0, maximum = Number.MAX_SAFE_INTEGER) {
  if (!Number.isSafeInteger(value) || value < minimum || value > maximum) reject("INVALID_VISUAL_PLAN", `Invalid ${name}`);
  return value;
}
export function natural(value, name) {
  if (typeof value !== "string" || !/^(0|[1-9][0-9]*)$/.test(value) || value.length > 20) reject("INVALID_VISUAL_PLAN", `Invalid ${name}`);
  return BigInt(value);
}
export function finite(value, name) {
  if (typeof value !== "number" || !Number.isFinite(value) || !Number.isFinite(Math.fround(value))) reject("INVALID_VISUAL_PLAN", `Invalid ${name}`);
  return value;
}
export function checkAbort(signal) {
  if (signal?.aborted) throw signal.reason ?? new VisualExecutionError("CANCELLED", "Visual execution cancelled");
}
export function byteBudget(maximum) {
  const limit = natural(maximum, "byte limit"); let used = 0n, peak = 0n;
  return { reserve(bytes) {
    const size = BigInt(integer(bytes, "allocation bytes"));
    if (size > limit - used) reject("VISUAL_BUDGET_EXCEEDED", "Visual allocation exceeds its admitted byte limit");
    used += size; peak = used > peak ? used : peak; let released = false;
    return () => { if (!released) { released = true; used -= size; } };
  }, get used() { return used; }, get peak() { return peak; } };
}

// JSON objects have no member order. Ordered arrays and primitive values retain
// exact semantics; this comparison never normalizes domain clocks or geometry.
export function sameJsonFacts(left, right) {
  if (left === right) return true;
  if (left === null || right === null || typeof left !== "object" || typeof right !== "object") return false;
  if (Array.isArray(left) || Array.isArray(right)) {
    return Array.isArray(left) && Array.isArray(right) && left.length === right.length && left.every((value, index) => sameJsonFacts(value, right[index]));
  }
  const keys = Object.keys(left);
  return keys.length === Object.keys(right).length && keys.every(key => Object.prototype.hasOwnProperty.call(right, key) && sameJsonFacts(left[key], right[key]));
}
