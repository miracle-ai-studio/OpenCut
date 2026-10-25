import {
  createContext,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { useEditingWorkspace } from "../../context";
import type { WorkspaceState } from "../../controller";
import { editRecoveryBlocked } from "../edit-recovery";
import type { Operation } from "../types";
import type { Time } from "../../../model/types";
import { compareTime, timeFromDecimalSeconds, zero } from "../../../model/time";
import { t } from "../../../shared/locale";
import { useResourceCopy } from "./resources-copy";
import { useTransitionCopy } from "./transition-copy";
import {
  sameTransitionTarget,
  transitionLimit,
  transitionPairs,
  type TransitionTarget,
} from "./transition-target";

function block(state: WorkspaceState) {
  const scene = state.scene;
  if (!state.api || !scene || state.busy) return "wait";
  if (scene.historical) return "history";
  if (editRecoveryBlocked(scene.recovery)) return "recovery";
  if (
    scene.historyPlan ||
    scene.gainDraft ||
    scene.audioGain ||
    scene.sourceAudio ||
    scene.media.attempt ||
    scene.timelineEdit.attempt ||
    scene.timelineEdit.recoveryError ||
    scene.clipboard.attempt
  )
    return "draft";
  return null;
}
function useTransitionState() {
  const { controller, api, scene } = useEditingWorkspace();
  const c = useTransitionCopy(),
    r = useResourceCopy();
  const snapshot = scene?.historical ?? scene?.head;
  const sequence = snapshot?.project.sequences.find(
    (item) => item.id === scene?.sequenceId,
  );
  const [selected, setSelected] = useState<TransitionTarget | null>(null);
  const [pending, setPending] = useState(false),
    [error, setError] = useState<string | null>(null),
    [notice, setNotice] = useState<string | null>(null);
  const latest = useRef(selected),
    writing = useRef(false),
    epoch = useRef(0),
    read = useRef<AbortController | null>(null);
  const pairs = sequence ? transitionPairs(sequence) : [];
  const pair =
    !scene?.selectedId && !scene?.timelineEdit.selection.length
      ? (pairs.find((item) => sameTransitionTarget(item.target, selected)) ??
        null)
      : null;
  const target = pair?.target ?? null;
  latest.current = target;
  const existing =
    pair &&
    sequence?.transitions.find(
      (item) =>
        item.outgoing_element_id === pair.outgoing.id &&
        item.incoming_element_id === pair.incoming.id,
    );
  const limit = pair
    ? transitionLimit(pair, snapshot?.project.assets ?? [])
    : null;
  const scope = JSON.stringify([
    api?.connectionIdentity,
    snapshot?.project.id,
    scene?.sequenceId,
    scene?.historical?.revision,
  ]);
  useEffect(() => {
    epoch.current++;
    read.current?.abort();
    setSelected(null);
    setError(null);
    setNotice(null);
  }, [scope]);
  useEffect(() => {
    if (!pair && selected) {
      epoch.current++;
      read.current?.abort();
      setSelected(null);
      setError(null);
      setNotice(null);
    }
  }, [pair, selected]);
  useEffect(
    () => () => {
      epoch.current++;
      read.current?.abort();
    },
    [],
  );
  const select = (next: TransitionTarget | null) => {
    epoch.current++;
    read.current?.abort();
    latest.current = next;
    controller.scene.select(null);
    setSelected(next);
    setError(null);
    setNotice(null);
  };
  const blocked = block(controller.getSnapshot());
  const disabledReason = blocked
    ? c(blocked)
    : !pair
      ? c("choose")
      : limit?.reason
        ? c(limit.reason)
        : null;
  const durationReason = (duration: Time | null) =>
    disabledReason ??
    (!duration || compareTime(duration, zero) <= 0
      ? r("positiveDuration")
      : limit?.maximum && compareTime(duration, limit.maximum) > 0
        ? c("tooLong")
        : null);
  const run = async (duration: Time | null, remove = false) => {
    if (
      !api ||
      !scene ||
      !sequence ||
      !pair ||
      writing.current ||
      blocked ||
      (remove ? !existing || pair.track.locked : durationReason(duration))
    )
      return;
    const serial = epoch.current,
      start = controller.getSnapshot(),
      originalTarget = pair.target;
    writing.current = true;
    setPending(true);
    setError(null);
    setNotice(null);
    const abort = new AbortController();
    read.current = abort;
    try {
      const check = () => {
        const now = controller.getSnapshot();
        if (
          abort.signal.aborted ||
          serial !== epoch.current ||
          now.api !== api ||
          !now.scene ||
          now.scene.head.project.id !== scene.head.project.id ||
          now.scene.head.revision !== scene.head.revision ||
          now.scene.sequenceId !== sequence.id ||
          block(now) ||
          now.scene.selectedId ||
          now.scene.timelineEdit.selection.length ||
          !sameTransitionTarget(latest.current, originalTarget)
        )
          throw new Error(c("changed"));
      };
      check();
      const operation: Operation = remove
        ? {
            type: "transition_delete",
            sequence_id: sequence.id,
            transition_id: existing!.id,
          }
        : {
            type: "transition_put",
            sequence_id: sequence.id,
            transition: {
              id: existing?.id ?? crypto.randomUUID(),
              outgoing_element_id: pair.outgoing.id,
              incoming_element_id: pair.incoming.id,
              duration: duration!,
              transition: "legacy_crossfade_v3",
            },
          };
      const request = api.scene.request(
        start.scene!.head,
        remove
          ? "Remove transition"
          : existing
            ? "Replace transition"
            : "Add transition",
        [operation],
      );
      await api.sdk.call("opencut_edit_plan", request, {
        signal: abort.signal,
      });
      check();
      await controller.scene.applyPlanned(request, sequence.id);
      if (serial === epoch.current)
        setNotice(remove ? c("removed") : r("transitionUpdated"));
    } catch (cause) {
      if (serial === epoch.current && !abort.signal.aborted)
        setError(t(cause instanceof Error ? cause.message : String(cause)));
    } finally {
      writing.current = false;
      if (read.current === abort) {
        read.current = null;
        setPending(false);
      }
    }
  };
  return {
    pair,
    target,
    existing,
    limit,
    select,
    pending,
    error,
    notice,
    revision: snapshot?.revision,
    disabledReason,
    durationReason,
    apply: (duration: Time) => void run(duration),
    remove: () => void run(null, true),
    removeDisabled: pending || !!blocked || !!pair?.track.locked,
  };
}
type TransitionState = ReturnType<typeof useTransitionState>;
const Context = createContext<TransitionState | null>(null);
export function TransitionProvider({ children }: { children: ReactNode }) {
  const value = useTransitionState();
  return <Context.Provider value={value}>{children}</Context.Provider>;
}
export const useTransitions = () => useContext(Context);
export function parseTransitionDuration(value: string) {
  try {
    return timeFromDecimalSeconds(value);
  } catch {
    return null;
  }
}
