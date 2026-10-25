import {addTime,compareTime,zero} from "../../model/time";
import type {Sequence} from "./types";
import type {Time} from "../../model/types";

/** Exact UI navigation bound, not a media clock or composition evaluator.
 * The application still validates every requested playback position. */
export function sequenceContentEnd(sequence:Sequence):Time {
  return [...sequence.tracks.flatMap(track=>track.elements.map(element=>element.timeline_range)),
    ...sequence.captions.map(caption=>caption.timeline_range)].reduce((end,range)=>{
    const candidate=addTime(range.start,range.duration);
    return compareTime(candidate,end)>0?candidate:end;
  },zero);
}
