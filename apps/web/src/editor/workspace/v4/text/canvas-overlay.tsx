import { useEffect, useRef, useState } from "react";
import { useWorkspace } from "../../context";
import { useCanvasTextAuthoring } from "./workspace";
import type { CanvasTextPlan } from "./canvas-types";
import { canvasTextGeometry, canvasTextViewport } from "./canvas-geometry";
import { secondsForDisplay } from "../../../model/time";
import { t } from "../../../shared/locale";
import { CanvasTextInput } from "./canvas-input";
/** Native scene pixels remain the preview. This layer only supplies text input
 * after a fixed-revision application plan has been admitted. */
export function CanvasTextOverlay() {
  const authoring = useCanvasTextAuthoring();
  const { api, v4 } = useWorkspace();
  const ref = useRef<HTMLDivElement>(null);
  const [viewport, setViewport] = useState({ width: 0, height: 0 });
  const [targets, setTargets] = useState<CanvasTextPlan[]>([]);
  const planner = authoring?.planCanvasText;
  const plannerRef = useRef(planner);
  plannerRef.current = planner;
  const hasPlanner = !!planner;
  // Audio owns the continuous sound-consumption clock. Loading/stalled and
  // uncertain in-flight control phases must not start extra font/plan work.
  const playbackBusy =
    !!v4 &&
    ["loading", "playing", "stalled", "unconfirmed"].includes(
      v4.audioPreview.phase,
    );
  const revision = v4?.head.revision,
    sequenceId = v4?.sequenceId;
  const timeStamp = JSON.stringify(v4?.playhead);
  useEffect(() => {
    setTargets([]);
    const planTarget = plannerRef.current;
    if (
      !planTarget ||
      !v4 ||
      v4.historical ||
      authoring?.disabled ||
      playbackBusy
    )
      return;
    const abort = new AbortController();
    const sequence = v4.head.project.sequences.find(
      (scene) => scene.id === v4.sequenceId,
    );
    const time = secondsForDisplay(v4.playhead);
    const elements =
      sequence?.tracks
        .filter((track) => track.enabled && !track.locked)
        .flatMap((track) => track.elements)
        .filter((element) => {
          const start = secondsForDisplay(element.timeline_range.start);
          return (
            element.enabled &&
            element.content.kind === "text" &&
            time >= start &&
            time < start + secondsForDisplay(element.timeline_range.duration)
          );
        }) ?? [];
    void (async () => {
      const plans: CanvasTextPlan[] = [];
      for (const element of elements) {
        const scope = {
          projectId: v4.head.project.id,
          revision: v4.head.revision,
          sequenceId: v4.sequenceId,
          elementId: element.id,
        };
        try {
          abort.signal.throwIfAborted();
          const plan = await planTarget(scope, v4.playhead, abort.signal);
          abort.signal.throwIfAborted();
          if (
            plan.scope.projectId === scope.projectId &&
            plan.scope.revision === scope.revision &&
            plan.scope.sequenceId === scope.sequenceId &&
            plan.layer.elementId === element.id
          )
            plans.push(plan);
        } catch {
          if (abort.signal.aborted) return;
          // One inactive/missing-font/refused title cannot suppress other
          // authorized text targets in the same immutable scene.
        }
      }
      abort.signal.throwIfAborted();
      setTargets(plans);
    })().catch(() => {
      /* Missing font/capability never creates a fallback hit box. */
    });
    return () => abort.abort();
  }, [
    hasPlanner,
    api,
    v4?.head.project.id,
    playbackBusy,
    revision,
    sequenceId,
    timeStamp,
    v4?.historical,
    authoring?.disabled,
  ]);
  useEffect(() => {
    const node = ref.current;
    if (!node) return;
    const update = () => {
      const rect = node.getBoundingClientRect();
      setViewport({ width: rect.width, height: rect.height });
    };
    update();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(update);
    observer.observe(node);
    return () => observer.disconnect();
  }, []);
  return (
    <div
      ref={ref}
      style={{ position: "absolute", inset: 0, pointerEvents: "none" }}
    >
      {!playbackBusy &&
        !authoring?.canvas &&
        targets.map((plan) => {
          const fitted = canvasTextViewport(plan, viewport);
          const geometry = canvasTextGeometry(plan, fitted.width);
          return (
            <div
              key={plan.layer.elementId}
              className="v4-canvas-text-position"
              style={{
                transform: geometry.transform,
                left: fitted.left,
                top: fitted.top,
              }}
            >
              <button
                type="button"
                className="v4-canvas-text-hit"
                aria-label={`${t("Edit text on canvas")}: ${plan.layer.text.content}`}
                style={{
                  left: geometry.left,
                  top: geometry.top,
                  width: geometry.width,
                  height: geometry.height,
                }}
                onDoubleClick={(event) => {
                  event.stopPropagation();
                  authoring?.beginCanvasEdit(plan.layer.elementId);
                }}
              />
            </div>
          );
        })}
      {authoring?.canvas &&
        api &&
        viewport.width > 0 &&
        (() => {
          const plan = authoring.canvas;
          const fitted = canvasTextViewport(plan, viewport);
          return (
            <div
              style={{
                position: "absolute",
                left: fitted.left,
                top: fitted.top,
                width: fitted.width,
                height: fitted.height,
              }}
            >
              <CanvasTextInput
                key={JSON.stringify(plan.scope)}
                plan={plan}
                api={api}
                viewportWidth={fitted.width}
                commit={authoring.commitCanvas}
                cancel={authoring.cancelCanvas}
              />
            </div>
          );
        })()}
    </div>
  );
}
