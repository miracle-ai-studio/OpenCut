import { Layers, Plus } from "lucide-react";
import { secondsForDisplay } from "../../../model/time";
import type { Sequence } from "../types";
import { useTransitionCopy } from "./transition-copy";
import { transitionPairs, sameTransitionTarget } from "./transition-target";
import { useTransitions } from "./transition-workspace";
import { useWorkbenchView } from "./view";

/** The hit area straddles the exact cut coordinate, without changing clip time. */
export function TransitionSeams({
  sequence,
  trackId,
  scale,
  disabled = false,
}: {
  sequence: Sequence;
  trackId: string;
  scale: number;
  disabled?: boolean;
}) {
  const state = useTransitions(),
    copy = useTransitionCopy(),
    view = useWorkbenchView();
  if (!state) return null;
  return transitionPairs(sequence)
    .filter((pair) => pair.track.id === trackId)
    .map((pair) => {
      const existing = sequence.transitions.find(
        (item) =>
          item.outgoing_element_id === pair.outgoing.id &&
          item.incoming_element_id === pair.incoming.id,
      );
      const name = `${copy("seam")} · ${pair.outgoing.name} → ${pair.incoming.name}`;
      return (
        <button
          key={JSON.stringify(pair.target)}
          type="button"
          className="timeline-transition-seam"
          data-timeline-no-gesture
          data-has-transition={!!existing}
          aria-label={name}
          title={name}
          aria-pressed={sameTransitionTarget(state.target, pair.target)}
          disabled={disabled || state.pending}
          style={{
            left: secondsForDisplay(pair.incoming.timeline_range.start) * scale,
          }}
          onClick={(event) => {
            event.stopPropagation();
            state.select(pair.target);
            view.showProperties();
          }}
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              event.stopPropagation();
              state.select(null);
            }
          }}
        >
          {existing ? <Layers size={13} /> : <Plus size={13} />}
        </button>
      );
    });
}
