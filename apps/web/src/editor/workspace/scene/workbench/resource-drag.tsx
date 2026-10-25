import {
  createContext,
  useContext,
  useEffect,
  useRef,
  useState,
  type DragEvent,
  type ReactNode,
} from "react";
import { useEditingWorkspace } from "../../context";
import { editRecoveryBlocked } from "../edit-recovery";
import { placeMediaDirect } from "../media-dialog";
import { useSceneStack } from "../scene-stack";
import { newEffectTrack } from "../scene-stack/model";
import { presetBlur } from "./resource-presets";
import { useTransitions } from "./transition-workspace";
import {
  resourceDrop,
  type DragResource,
  type ResourceHit,
  type ResourceDrop,
  type ResourcePosition,
} from "./resource-drop-model";
import type { EditorApi } from "../../api";
import type { Operation } from "../types";
import { addTime, halfTime } from "../../../model/time";
import { t } from "../../../shared/locale";

export const RESOURCE_DRAG_TYPE = "application/x-opencut-workbench-resource";
type Intention = {
  id: string;
  resource: DragResource;
  scope: Omit<ResourcePosition, "trackId" | "at">;
  api: EditorApi;
};
function useResourceDragState() {
  const { controller, api, scene, busy } = useEditingWorkspace(),
    stack = useSceneStack(),
    transitions = useTransitions();
  const [drag, setDrag] = useState<Intention | null>(null),
    [target, setTarget] = useState<ResourceDrop | null>(null),
    [pending, setPending] = useState(false),
    [error, setError] = useState<string | null>(null);
  const active = useRef<Intention | null>(null),
    writing = useRef(false);
  const scopeKey = JSON.stringify([
    api?.connectionIdentity,
    scene?.head.project.id,
    scene?.head.revision,
    scene?.sequenceId,
    scene?.historical?.revision,
  ]);
  const blocked = () => {
    const state = controller.getSnapshot(),
      scene = state.scene;
    return (
      !state.api ||
      !scene ||
      state.busy ||
      !!scene.propertyDrafts?.length ||
      !!scene.historical ||
      editRecoveryBlocked(scene.recovery) ||
      !!(
        scene.historyPlan ||
        scene.gainDraft ||
        scene.audioGain ||
        scene.sourceAudio ||
        scene.timelineEdit.attempt ||
        scene.timelineEdit.recoveryError ||
        scene.media.attempt ||
        scene.clipboard.attempt
      )
    );
  };
  const cancel = () => {
    active.current = null;
    setDrag(null);
    setTarget(null);
  };
  useEffect(() => {
    cancel();
    setError(null);
  }, [scopeKey]);
  useEffect(() => {
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape" && active.current) {
        event.preventDefault();
        event.stopPropagation();
        cancel();
      }
    };
    const visibility = () => {
      if (document.hidden) cancel();
    };
    window.addEventListener("keydown", escape, true);
    window.addEventListener("blur", cancel);
    window.addEventListener("dragend", cancel);
    document.addEventListener("visibilitychange", visibility);
    return () => {
      window.removeEventListener("keydown", escape, true);
      window.removeEventListener("blur", cancel);
      window.removeEventListener("dragend", cancel);
      document.removeEventListener("visibilitychange", visibility);
    };
  }, []);
  const current = (intent: Intention) => {
    const state = controller.getSnapshot(),
      scene = state.scene;
    if (
      state.api !== intent.api ||
      !scene ||
      scene.historical ||
      scene.head.project.id !== intent.scope.projectId ||
      scene.head.revision !== intent.scope.revision ||
      scene.sequenceId !== intent.scope.sequenceId
    )
      throw Error("The drop target changed.");
    return {
      state,
      scene,
      sequence: scene.head.project.sequences.find(
        (item) => item.id === scene.sequenceId,
      )!,
    };
  };
  const start = (event: DragEvent, resource: DragResource) => {
    if (
      !api ||
      !scene ||
      blocked() ||
      writing.current ||
      (resource.kind === "effect" && (!stack?.available || stack.disabled))
    ) {
      event.preventDefault();
      return;
    }
    const intent = {
      id: crypto.randomUUID(),
      resource,
      api,
      scope: {
        projectId: scene.head.project.id,
        revision: scene.head.revision,
        sequenceId: scene.sequenceId,
      },
    };
    active.current = intent;
    setDrag(intent);
    setError(null);
    setTarget(null);
    // Only this document's live intent can be consumed. DataTransfer is not an
    // SDK request, asset grant, external path, or executable callback.
    event.dataTransfer.setData(RESOURCE_DRAG_TYPE, intent.id);
    event.dataTransfer.effectAllowed = "copy";
  };
  const hover = (hit: ResourceHit | null) => {
    if (!hit || !active.current) {
      setTarget(null);
      return null;
    }
    const intent = active.current;
    let result: ResourceDrop;
    try {
      const { scene, sequence } = current(intent);
      result = resourceDrop(
        intent.resource,
        sequence,
        scene.head.project.assets,
        hit,
      );
      if (blocked()) result.reason = "Finish or cancel the current edit first.";
    } catch {
      result = {
        kind: "media",
        trackId: hit.trackId,
        at: hit.at,
        duration: null,
        name: "",
        reason: "The drop target changed.",
      };
    }
    setTarget(result);
    return result;
  };
  const drop = async (hit: ResourceHit, token: string) => {
    const intent = active.current;
    if (!intent || token !== intent.id || writing.current) return;
    const chosen = hover(hit);
    if (!chosen || chosen.reason || blocked()) {
      setError(chosen?.reason ?? "The drop target changed.");
      cancel();
      return;
    }
    const { scene: captured, sequence } = current(intent),
      resource = intent.resource;
    cancel();
    writing.current = true;
    setPending(true);
    setError(null);
    try {
      let elementId: string | undefined,
        at = chosen.at;
      if (resource.kind === "media") {
        const asset = captured.head.project.assets.find(
          (item) => item.id === resource.assetId,
        )!;
        const result = await placeMediaDirect(controller, asset, {
          ...intent.scope,
          trackId: chosen.trackId,
          at,
        });
        elementId = result?.elementId;
      } else if (resource.kind === "catalog-audio") {
        const result = await resource.place({ ...intent.scope, trackId: chosen.trackId, at });
        elementId = result?.elementId;
      } else {
        const operations: Operation[] = [];
        if (resource.kind === "transition" && chosen.seam) {
          const seam = chosen.seam,
            existing = sequence.transitions.find(
              (item) =>
                item.outgoing_element_id === seam.outgoingId &&
                item.incoming_element_id === seam.incomingId,
            );
          operations.push({
            type: "transition_put",
            sequence_id: sequence.id,
            transition: {
              id: existing?.id ?? crypto.randomUUID(),
              outgoing_element_id: seam.outgoingId,
              incoming_element_id: seam.incomingId,
              transition: "legacy_crossfade_v3",
              duration: resource.duration,
            },
          });
          at = addTime(at, halfTime(resource.duration));
        } else if (
          resource.kind === "effect" &&
          chosen.kind === "effect-clip"
        ) {
          const track = sequence.tracks.find(
              (item) => item.id === chosen.trackId,
            )!,
            element = structuredClone(
              track.elements.find((item) => item.id === chosen.elementId)!,
            );
          element.visual!.effects.push(presetBlur(resource.intensity));
          elementId = element.id;
          operations.push({
            type: "element_put",
            sequence_id: sequence.id,
            track_id: track.id,
            element,
          });
        } else if (resource.kind === "effect") {
          const track = newEffectTrack(at, resource.duration);
          track.name = t("Blur effect layer");
          track.elements[0].name = resource.name;
          if (track.elements[0].content.kind === "effect_layer")
            track.elements[0].content.effect = presetBlur(resource.intensity);
          elementId = track.elements[0].id;
          operations.push(
            chosen.trackId
              ? {
                  type: "element_put",
                  sequence_id: sequence.id,
                  track_id: chosen.trackId,
                  element: track.elements[0],
                }
              : { type: "track_put", sequence_id: sequence.id, track },
          );
        }
        current(intent);
        const request = intent.api.scene.request(
          captured.head,
          resource.kind === "transition"
            ? chosen.replace
              ? "Replace transition"
              : "Add transition"
            : "Add blur",
          operations,
        );
        await intent.api.sdk.call("opencut_edit_plan", request);
        current(intent);
        if (blocked()) throw Error("Finish or cancel the current edit first.");
        await controller.scene.applyPlanned(request, sequence.id);
      }
      const after = controller.getSnapshot();
      if (
        after.api === intent.api &&
        after.scene?.head.project.id === intent.scope.projectId &&
        after.scene.sequenceId === intent.scope.sequenceId &&
        !after.scene.historical
      ) {
        if (chosen.seam) transitions?.select(chosen.seam);
        else if (elementId) controller.scene.select(elementId);
        controller.scene.seek(at);
      }
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      writing.current = false;
      setPending(false);
    }
  };
  return {
    drag,
    target,
    pending,
    error,
    start,
    cancel,
    hover,
    drop,
    disabled: busy || blocked() || pending,
  };
}
const Context = createContext<ReturnType<typeof useResourceDragState> | null>(
  null,
);
export function ResourceDragProvider({ children }: { children: ReactNode }) {
  return (
    <Context.Provider value={useResourceDragState()}>
      {children}
    </Context.Provider>
  );
}
export const useResourceDrag = () => useContext(Context);
