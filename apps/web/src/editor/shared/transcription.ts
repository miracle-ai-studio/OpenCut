export function transcriptionFailure(code: string) {
  if (code === "CAPABILITY_UNAVAILABLE")
    return "Speech recognition is not ready on this computer.";
  if (
    code.startsWith("STORAGE_") ||
    code === "MEDIA_OUTPUT_LIMIT_EXCEEDED" ||
    code === "DATABASE_CAPACITY_EXHAUSTED"
  )
    return "There is not enough available storage to recognize this audio.";
  return "Speech recognition could not finish. Try again or use another audio clip.";
}
