// Classic persisted ticks decoder, matching its historical half-away-from-zero rule.
export function roundMediaTime({ time }: { time: number }) {
  const magnitude = Math.round(Math.abs(time)),
    value = magnitude === 0 ? 0 : time < 0 ? -magnitude : magnitude;
  if (!Number.isSafeInteger(value))
    throw new Error("Invalid Classic media ticks");
  return value;
}
