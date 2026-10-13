/** Rust's wire budget is a decimal u64; the execution adapter consumes a JS
 * allocation count. Reject lossy conversion before the renderer allocates. */
export function previewExecutionAdmission<T extends { maxEncodedBytes: string }>(
  admission: T,
): Omit<T, "maxEncodedBytes"> & { maxEncodedBytes: number } {
  const value = admission.maxEncodedBytes;
  if (!/^[1-9][0-9]*$/.test(value) || BigInt(value) > BigInt(Number.MAX_SAFE_INTEGER))
    throw new Error("Invalid encoded byte admission");
  return { ...admission, maxEncodedBytes: Number(value) };
}
