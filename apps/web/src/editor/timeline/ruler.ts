// Adapted from the Classic editor ruler; frame and label spacing are presentation only.
import type { Sequence } from "../model/types";
type FrameRate = Sequence["fps"];
import { PIXELS_PER_SECOND as BASE_TIMELINE_PIXELS_PER_SECOND } from "./geometry";
const frameRateToFloat = (fps: FrameRate) => fps.numerator / fps.denominator;

const LABEL_FRAME_INTERVALS = [2, 3, 5, 10, 15] as const;

const TICK_FRAME_INTERVALS = [1, 2, 3, 5, 10, 15] as const;

const SECOND_MULTIPLIERS = [
  1, 2, 3, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600,
] as const;

const MIN_LABEL_SPACING_PX = 120;

const MIN_TICK_SPACING_PX = 18;

export interface RulerConfig {
  labelIntervalSeconds: number;
  tickIntervalSeconds: number;
}

export function getRulerConfig({
  zoomLevel,
  fps,
}: {
  zoomLevel: number;
  fps: FrameRate;
}): RulerConfig {
  const fpsFloat = frameRateToFloat(fps);
  const pixelsPerSecond = BASE_TIMELINE_PIXELS_PER_SECOND * zoomLevel;
  const pixelsPerFrame = pixelsPerSecond / fpsFloat;

  const labelIntervalSeconds = findOptimalInterval({
    pixelsPerFrame,
    pixelsPerSecond,
    fps: fpsFloat,
    minSpacingPx: MIN_LABEL_SPACING_PX,
    frameIntervals: LABEL_FRAME_INTERVALS,
  });

  const rawTickIntervalSeconds = findOptimalInterval({
    pixelsPerFrame,
    pixelsPerSecond,
    fps: fpsFloat,
    minSpacingPx: MIN_TICK_SPACING_PX,
    frameIntervals: TICK_FRAME_INTERVALS,
  });

  // ensure tick interval divides evenly into label interval so labels always land on ticks
  const tickIntervalSeconds = ensureTickDividesLabel({
    tickIntervalSeconds: rawTickIntervalSeconds,
    labelIntervalSeconds,
    pixelsPerFrame,
    pixelsPerSecond,
    fps: fpsFloat,
  });

  return { labelIntervalSeconds, tickIntervalSeconds };
}

function ensureTickDividesLabel({
  tickIntervalSeconds,
  labelIntervalSeconds,
  pixelsPerFrame,
  pixelsPerSecond,
  fps,
}: {
  tickIntervalSeconds: number;
  labelIntervalSeconds: number;
  pixelsPerFrame: number;
  pixelsPerSecond: number;
  fps: number;
}): number {
  const labelFrames = Math.round(labelIntervalSeconds * fps);
  const tickFrames = Math.round(tickIntervalSeconds * fps);

  // if tick already divides label evenly, we're good
  if (labelFrames % tickFrames === 0) {
    return tickIntervalSeconds;
  }

  // find the smallest tick interval that divides the label interval and has adequate spacing
  for (const candidateFrames of TICK_FRAME_INTERVALS) {
    if (labelFrames % candidateFrames === 0) {
      const candidateSpacing = pixelsPerFrame * candidateFrames;
      // accept if spacing meets minimum threshold
      if (candidateSpacing >= MIN_TICK_SPACING_PX) {
        return candidateFrames / fps;
      }
    }
  }

  // try second-level tick intervals that divide the label interval cleanly
  for (const candidateSeconds of SECOND_MULTIPLIERS) {
    const ratio = labelIntervalSeconds / candidateSeconds;
    const isDivisor = Math.abs(ratio - Math.round(ratio)) < 0.0001;
    if (isDivisor) {
      const candidateSpacing = pixelsPerSecond * candidateSeconds;
      if (candidateSpacing >= MIN_TICK_SPACING_PX) {
        return candidateSeconds;
      }
    }
  }

  // fallback: use the label interval itself (no intermediate ticks)
  return labelIntervalSeconds;
}

function findOptimalInterval({
  pixelsPerFrame,
  pixelsPerSecond,
  fps,
  minSpacingPx,
  frameIntervals,
}: {
  pixelsPerFrame: number;
  pixelsPerSecond: number;
  fps: number;
  minSpacingPx: number;
  frameIntervals: readonly number[];
}): number {
  // try frame-level intervals first
  for (const frameInterval of frameIntervals) {
    const pixelSpacing = pixelsPerFrame * frameInterval;
    if (pixelSpacing >= minSpacingPx) {
      return frameInterval / fps;
    }
  }

  // then try second-level intervals
  for (const secondMultiplier of SECOND_MULTIPLIERS) {
    const pixelSpacing = pixelsPerSecond * secondMultiplier;
    if (pixelSpacing >= minSpacingPx) {
      return secondMultiplier;
    }
  }

  return 60;
}

export function shouldShowLabel({
  time,
  labelIntervalSeconds,
}: {
  time: number;
  labelIntervalSeconds: number;
}): boolean {
  const epsilon = 0.0001;
  const remainder = time % labelIntervalSeconds;
  return remainder < epsilon || remainder > labelIntervalSeconds - epsilon;
}

export function formatRulerLabel({
  timeInSeconds,
  fps,
}: {
  timeInSeconds: number;
  fps: FrameRate;
}): string {
  if (isSecondBoundary({ timeInSeconds })) {
    return formatTimestamp({ timeInSeconds });
  }

  const frameWithinSecond = getFrameWithinSecond({
    timeInSeconds,
    fps: frameRateToFloat(fps),
  });
  return `${frameWithinSecond}f`;
}

function isSecondBoundary({
  timeInSeconds,
}: {
  timeInSeconds: number;
}): boolean {
  const epsilon = 0.0001;
  const remainder = timeInSeconds % 1;
  return remainder < epsilon || remainder > 1 - epsilon;
}

function getFrameWithinSecond({
  timeInSeconds,
  fps,
}: {
  timeInSeconds: number;
  fps: number;
}): number {
  const fractionalPart = timeInSeconds % 1;
  return Math.round(fractionalPart * fps);
}

function formatTimestamp({ timeInSeconds }: { timeInSeconds: number }): string {
  const totalSeconds = Math.round(timeInSeconds);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;

  const mm = minutes.toString().padStart(2, "0");
  const ss = seconds.toString().padStart(2, "0");

  if (hours > 0) {
    return `${hours}:${mm}:${ss}`;
  }

  return `${mm}:${ss}`;
}
