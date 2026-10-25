import type { Time } from "../../../model/types";
import { compareTime, zero } from "../../../model/time";
import type { Sequence } from "../types";
import type { TimelineCommand, TimelineScope, TimelineTarget } from "./types";
import { moveSelectionCommand } from "./actions";
import {
  moveDeltaAtEdge,
  pixelsPerSecond,
  pointerTime,
  sequenceEdges,
  signedPointerDelta,
} from "./geometry";

export type GestureIdentity = {
  api: object | null;
  generation: number;
  viewGeneration: number;
};
export type GestureFence = GestureIdentity & TimelineScope;
export function sameGestureFence(a: GestureFence, b: GestureFence): boolean {
  return (
    a.api === b.api &&
    a.generation === b.generation &&
    a.viewGeneration === b.viewGeneration &&
    a.projectId === b.projectId &&
    a.revision === b.revision &&
    a.sequenceId === b.sequenceId
  );
}
export type GestureStart = {
  fence: GestureFence;
  sequence: Sequence;
  selected: TimelineTarget[];
  primary: TimelineTarget;
  kind: "move" | "left" | "right";
  pointerId: number;
  clientX: number;
  scrollLeft: number;
  zoom: number;
  ripple: boolean;
  snapping?: boolean;
};
export type GestureDraft = {
  command: TimelineCommand | null;
  changed: boolean;
  error: string | null;
  delta: Time;
  trackOffset: number;
};

/** One captured pointer intention. End is consumptive, so a capture-loss event
 * after pointerup cannot schedule a second plan. Cancellation produces no command. */
export class TimelineGesture {
  private active = true;
  hasMoved = false;
  private draft: GestureDraft = {
    command: null,
    changed: false,
    error: null,
    delta: { ticks: "0", timebase: 1 },
    trackOffset: 0,
  };
  constructor(readonly start: GestureStart) {}
  update(
    clientX: number,
    scrollLeft: number,
    trackOffset: number,
    fence: GestureFence,
  ): GestureDraft | null {
    if (!this.active || !sameGestureFence(this.start.fence, fence)) {
      this.cancel();
      return null;
    }
    const dx =
      clientX - this.start.clientX + scrollLeft - this.start.scrollLeft;
    const changed =
      Math.abs(dx) >= 4 || (this.start.kind === "move" && trackOffset !== 0);
    this.hasMoved ||= changed;
    try {
      const raw = signedPointerDelta(dx / pixelsPerSecond(this.start.zoom));
      const delta =
        this.start.kind === "move" && this.start.snapping !== false
          ? moveDeltaAtEdge(
              this.start.sequence,
              this.start.primary,
              raw,
              this.start.zoom,
              this.start.selected,
            )
          : raw;
      const command =
        this.start.kind === "move"
          ? moveSelectionCommand(
              this.start.sequence,
              this.start.selected,
              delta,
              trackOffset,
              this.start.ripple,
            )
          : this.trimCommand(dx);
      const effective =
        changed &&
        (this.start.kind !== "move" ||
          compareTime(delta, zero) !== 0 ||
          trackOffset !== 0);
      this.draft = {
        command,
        changed: effective,
        error: null,
        delta,
        trackOffset,
      };
    } catch (error) {
      this.draft = {
        ...this.draft,
        command: null,
        changed,
        error: error instanceof Error ? error.message : String(error),
      };
    }
    return this.draft;
  }
  private trimCommand(dx: number): TimelineCommand {
    const track = this.start.sequence.tracks.find(
      (item) => item.id === this.start.primary.track_id,
    )!;
    if (track.locked) throw new Error("The selected track is locked.");
    const element =
      track.elements.find(
        (item) => item.id === this.start.primary.element_id,
      ) ??
      this.start.sequence.captions.find(
        (c) =>
          c.id === this.start.primary.element_id && c.track_id === track.id,
      );
    if (!element) throw new Error("The timeline target changed.");
    const range = element.timeline_range;
    const base =
      this.start.kind === "left"
        ? Number(range.start.ticks) / range.start.timebase
        : Number(range.start.ticks) / range.start.timebase +
          Number(range.duration.ticks) / range.duration.timebase;
    return {
      kind: "trim",
      target: this.start.primary,
      edge: this.start.kind === "left" ? "left" : "right",
      boundary: pointerTime(
        base + dx / pixelsPerSecond(this.start.zoom),
        sequenceEdges(this.start.sequence, [this.start.primary]),
        this.start.snapping === false
          ? 0
          : 8 / pixelsPerSecond(this.start.zoom),
      ),
      ripple: this.start.ripple,
    };
  }
  finish(fence: GestureFence): GestureDraft | null {
    if (!this.active || !sameGestureFence(this.start.fence, fence)) {
      this.cancel();
      return null;
    }
    this.active = false;
    return this.draft.changed ? this.draft : null;
  }
  cancel(): void {
    this.active = false;
  }
}
