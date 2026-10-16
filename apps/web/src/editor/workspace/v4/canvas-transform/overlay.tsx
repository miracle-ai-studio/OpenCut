import { useEffect, useRef, useState } from "react";
import type {
  PointerEvent as ReactPointerEvent,
  MouseEvent as ReactMouseEvent,
} from "react";
import { useWorkspace } from "../../context";
import type { EditorApi } from "../../api";
import type { V4SessionState } from "../types";
import {
  planTransforms,
  hasElementTransformAnimation,
  sampleTransforms,
  gestureIntent,
  gestureProblem,
  clearsScaleAnimation,
} from "./planning";
import type { TransformTarget, GestureIntent } from "./planning";
import { selectTimelineTarget } from "../timeline-edit/selection";
import { compareTime, addTime } from "../../../model/time";
import { fontRecoveryBlocked } from "../text/font-recovery";
import { canvasTextPlanner } from "../text/canvas-plan";
import { useCanvasTextAuthoring } from "../text/workspace";
import { canvasTextViewport } from "../text/canvas-geometry";
import {
  mediaBasis,
  textBasis,
  dragSelection,
  selectionOutline,
  groupRotationProblem,
} from "./geometry";
import type { Basis, Handle, Transform, XY } from "./geometry";
import { useCanvasText } from "./messages";
import "./transform.css";
type Capture = {
  api: EditorApi;
  connection: string | null;
  projectId: string;
  revision: string;
  sequenceId: string;
  timeKey: string;
  targets: TransformTarget[];
  selectionKey: string;
  sessionKey: string;
  serial: number;
};
type Plan = {
  capture: Capture;
  bases: Basis[];
  transforms: Transform[];
  v2: boolean;
};
const selectionIdentity = (view: V4SessionState) =>
  JSON.stringify([view.selectedId, view.timelineEdit.selection]);
const sessionIdentity = (view: V4SessionState) => {
  const s = view.audioPreview.session;
  return JSON.stringify(
    s && [
      s.identity,
      s.session_id,
      s.owner_id,
      s.lease_id,
      s.generation,
      s.overlay_epoch,
    ],
  );
};
type Gesture = {
  plan: Plan;
  pointer: number;
  node: Element;
  handle: Handle;
  intent?: GestureIntent;
  from: XY;
  original: Transform[];
  next: Transform[];
  draftId: string;
  version: number;
  rendering: boolean;
  previewAbort: AbortController | null;
  failed: boolean;
};
export function V4CanvasTransformOverlay({
  width,
  height,
  disabled = false,
}: {
  width: number;
  height: number;
  disabled?: boolean;
}) {
  const { api, v4, controller, busy } = useWorkspace(),
    textAuthoring = useCanvasTextAuthoring(),
    editingDisabled =
      disabled || !!textAuthoring?.canvas || !!textAuthoring?.disabled,
    t = useCanvasText();
  const [plan, setPlan] = useState<Plan | null>(null),
    [draft, setDraft] = useState<Transform[] | null>(null),
    [error, setError] = useState<string | null>(null),
    [pending, setPending] = useState(false),
    [viewport, setViewport] = useState({ width: 0, height: 0 });
  const hitInFlight = useRef<{
    pixel: XY;
    editText: boolean;
    current(): boolean;
  } | null>(null);
  const hitPress = useRef<{ pointer: number; x: number; y: number } | null>(
      null,
    ),
    hitSerial = useRef(0),
    container = useRef<HTMLDivElement>(null),
    serial = useRef(0),
    abort = useRef<AbortController | null>(null),
    gesture = useRef<Gesture | null>(null),
    draftOwner = useRef<Gesture | null>(null),
    writing = useRef(false),
    latest = useRef({ disabled: editingDisabled, textAuthoring });
  latest.current = { disabled: editingDisabled, textAuthoring };
  const blocked = () => {
    const s = controller.getSnapshot(),
      v = s.v4;
    return (
      latest.current.disabled ||
      !v ||
      s.busy ||
      !!v.historical ||
      !!v.gainDraft ||
      ["loading", "playing", "stalled", "unconfirmed"].includes(
        v.audioPreview.phase,
      ) ||
      fontRecoveryBlocked(v.fontRecovery) ||
      controller.v4.captionRecovery.blocked() ||
      !!v.recovery?.record ||
      !!v.recovery?.error ||
      ["reading", "staging", "applying"].includes(v.recovery?.phase ?? "") ||
      ["applying", "unconfirmed"].includes(
        v.timelineEdit.attempt?.phase ?? "",
      ) ||
      ["applying", "unconfirmed"].includes(v.clipboard.attempt?.phase ?? "")
    );
  };
  const current = (c: Capture) => {
    const s = controller.getSnapshot(),
      v = s.v4;
    if (
      blocked() ||
      serial.current !== c.serial ||
      s.api !== c.api ||
      s.api.connectionIdentity !== c.connection ||
      v?.head.project.id !== c.projectId ||
      v.head.revision !== c.revision ||
      v.sequenceId !== c.sequenceId ||
      selectionIdentity(v) !== c.selectionKey ||
      sessionIdentity(v) !== c.sessionKey ||
      JSON.stringify(v.playhead) !== c.timeKey
    )
      return false;
    const sequence = v.head.project.sequences.find(
      (s) => s.id === c.sequenceId,
    );
    return c.targets.every((target) => {
      const track = sequence?.tracks.find((t) => t.id === target.trackId),
        element = track?.elements.find((e) => e.id === target.element.id);
      return (
        !!track &&
        !track.locked &&
        track.enabled &&
        !!element?.enabled &&
        JSON.stringify(element) === JSON.stringify(target.element)
      );
    });
  };
  const clearDraft = (g: Gesture) => {
    g.previewAbort?.abort();
    controller.v4.visualPreview.clearTransformDraft(g.draftId);
    if (draftOwner.current === g) draftOwner.current = null;
  };
  const cancelGesture = () => {
    const g = gesture.current ?? draftOwner.current;
    gesture.current = null;
    if (g) clearDraft(g);
    if (g?.node.hasPointerCapture?.(g.pointer))
      g.node.releasePointerCapture(g.pointer);
    setDraft(null);
  };
  const invalidate = () => {
    hitSerial.current++;
    serial.current++;
    abort.current?.abort();
    cancelGesture();
    setPlan(null);
    if (!writing.current) setPending(false);
  };
  useEffect(() => {
    const node = container.current;
    if (!node) return;
    const update = () => {
      const r = node.getBoundingClientRect();
      setViewport((previous) =>
        previous.width === r.width && previous.height === r.height
          ? previous
          : { width: r.width, height: r.height },
      );
    };
    update();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(update);
    observer.observe(node);
    return () => observer.disconnect();
  }, []);
  const timeKey = JSON.stringify(v4?.playhead);
  useEffect(() => {
    invalidate();
    setError(null);
    const state = controller.getSnapshot(),
      view = state.v4;
    if (!api || !view || blocked() || !view.selectedId) return;
    const sequence = view.head.project.sequences.find(
      (s) => s.id === view.sequenceId,
    );
    const selected = view.timelineEdit.selection.length
      ? view.timelineEdit.selection
      : (sequence?.tracks.flatMap((track) =>
          track.elements
            .filter((e) => e.id === view.selectedId)
            .map((e) => ({ track_id: track.id, element_id: e.id })),
        ) ?? []);
    if (!sequence || !selected.length) return;
    if (
      selected.length > 60 ||
      new Set(selected.map((target) => target.element_id)).size !==
        selected.length
    ) {
      setError("The canvas selection is not a unique supported group.");
      return;
    }
    const targets: TransformTarget[] = [];
    for (const target of selected) {
      const track = sequence.tracks.find((t) => t.id === target.track_id),
        element = track?.elements.find((e) => e.id === target.element_id);
      if (
        !track ||
        track.locked ||
        !track.enabled ||
        !element?.enabled ||
        !element.visual ||
        !["video", "image", "graphic", "text", "sticker"].includes(
          element.content.kind,
        ) ||
        compareTime(view.playhead, element.timeline_range.start) < 0 ||
        compareTime(
          view.playhead,
          addTime(
            element.timeline_range.start,
            element.timeline_range.duration,
          ),
        ) >= 0
      ) {
        setError(
          "Choose enabled, unlocked visual elements active at the current time.",
        );
        return;
      }
      if (
        sequence.transitions.some(
          (t) =>
            t.outgoing_element_id === element.id ||
            t.incoming_element_id === element.id,
        )
      ) {
        setError(
          "Transform-animated or transitioning elements require the keyframe editor.",
        );
        return;
      }
      targets.push({ trackId: track.id, element: structuredClone(element) });
    }
    const capture: Capture = {
      api,
      connection: api.connectionIdentity,
      projectId: view.head.project.id,
      revision: view.head.revision,
      sequenceId: view.sequenceId,
      timeKey: JSON.stringify(view.playhead),
      targets,
      selectionKey: selectionIdentity(view),
      sessionKey: sessionIdentity(view),
      serial: serial.current,
    };
    const controllerAbort = new AbortController();
    abort.current = controllerAbort;
    setPending(true);
    void (async () => {
      try {
        const capabilities = await api.sdk.hostCapabilities();
        if (controllerAbort.signal.aborted || !current(capture)) return;
        const v2 =
          capabilities.tools.includes("opencut_element_transform_v2_sample") &&
          capabilities.tools.includes(
            "opencut_visual_preview_v4_transform_draft_v2_frame",
          );
        if (
          !v2 &&
          (!capabilities.tools.includes("opencut_element_properties_v1_plan") ||
            !capabilities.tools.includes(
              "opencut_visual_preview_v4_transform_draft_frame",
            ))
        )
          throw Error("Canvas transforms are unavailable on this host.");
        if (!v2 && targets.some((t) => hasElementTransformAnimation(t.element)))
          throw Error(
            "Transform-animated or transitioning elements require the keyframe editor.",
          );
        if (!v2 && targets.some((t) => t.element.content.kind === "sticker"))
          throw Error(
            "Sticker transforms require the sampled geometry capability.",
          );
        const sampled = v2
          ? await sampleTransforms(
              api,
              capture,
              view.playhead,
              targets,
              controllerAbort.signal,
              () => current(capture),
            )
          : null;
        const transforms = sampled
          ? sampled.map((entry) => entry.transform)
          : targets.map((t) => structuredClone(t.element.visual!.transform));
        if (transforms.some((t) => !t.scale_x || !t.scale_y))
          throw Error("Zero-sized elements cannot be edited on canvas.");
        const bases: Basis[] = [];
        for (const [index, { element }] of targets.entries()) {
          let basis: Basis;
          if (element.content.kind === "text")
            basis = textBasis(
              await canvasTextPlanner(api)(
                {
                  projectId: capture.projectId,
                  revision: capture.revision,
                  sequenceId: capture.sequenceId,
                  elementId: element.id,
                },
                view.playhead,
                controllerAbort.signal,
              ),
            );
          else if (sampled) {
            // V2 geometry belongs to the same Rust sample/head/time as its
            // transform, including animated graphic dimensions and stickers.
            // The legacy Scene geometry endpoint rejects animated elements.
            basis = mediaBasis(sampled[index].geometry!, transforms[index], {
              width,
              height,
            });
          } else {
            const result = await api.sdk.call(
              "opencut_scene_v1_geometry",
              {
                project_id: capture.projectId,
                expected_revision: capture.revision,
                sequence_id: capture.sequenceId,
                element_id: element.id,
                time: structuredClone(view.playhead),
              },
              { signal: controllerAbort.signal },
            );
            if (
              result.schema_version !== 1 ||
              result.revision !== capture.revision ||
              result.element_id !== element.id ||
              ![
                result.quad.width,
                result.quad.height,
                result.quad.centerX,
                result.quad.centerY,
                result.quad.rotationRadians,
              ].every(Number.isFinite) ||
              result.quad.width <= 0 ||
              result.quad.height <= 0
            )
              throw Error("Canvas geometry differs from the fixed target.");
            basis = mediaBasis(result, transforms[index], {
              width,
              height,
            });
          }
          if (controllerAbort.signal.aborted || !current(capture)) return;
          bases.push(basis);
        }
        setPlan({ capture, bases, transforms, v2 });
      } catch (cause) {
        if (!controllerAbort.signal.aborted && current(capture))
          setError(cause instanceof Error ? cause.message : String(cause));
      } finally {
        if (capture.serial === serial.current) setPending(false);
      }
    })();
    return () => invalidate();
  }, [
    api,
    api?.connectionIdentity,
    v4?.head.revision,
    v4?.head.project.id,
    v4?.historical?.revision,
    v4?.selectedId,
    JSON.stringify(v4?.timelineEdit.selection),
    v4 && sessionIdentity(v4),
    v4?.sequenceId,
    timeKey,
    v4?.audioPreview.phase,
    width,
    height,
    viewport.width,
    viewport.height,
    editingDisabled,
    busy,
    v4?.recovery,
    v4?.fontRecovery,
  ]);
  useEffect(() => {
    const cancel = () => {
      hitSerial.current++;
      hitPress.current = null;
      if (!writing.current) {
        abort.current?.abort();
        cancelGesture();
        setPending(false);
      }
    };
    const key = (event: KeyboardEvent) => {
      if (
        event.key === "Escape" &&
        (gesture.current || pending) &&
        !writing.current
      ) {
        event.preventDefault();
        event.stopPropagation();
        serial.current++;
        cancel();
        setPlan(null);
      }
    };
    window.addEventListener("blur", cancel);
    window.addEventListener("pagehide", cancel);
    window.addEventListener("keydown", key, true);
    return () => {
      window.removeEventListener("blur", cancel);
      window.removeEventListener("pagehide", cancel);
      window.removeEventListener("keydown", key, true);
    };
  }, [pending]);
  const fit = canvasTextViewport({ width, height }, viewport),
    scale = fit.width / width;
  const point = (event: { clientX: number; clientY: number }): XY => {
    const r = container.current!.getBoundingClientRect();
    return {
      x: (event.clientX - r.left - fit.left) / scale,
      y: (event.clientY - r.top - fit.top) / scale,
    };
  };
  const members = (p: Plan, transforms: readonly Transform[]) =>
    p.bases.map((basis, index) => ({ basis, transform: transforms[index] }));
  const renderDraft = async (g: Gesture) => {
    if (g.rendering) return;
    g.rendering = true;
    let presented = -1;
    try {
      while (
        gesture.current === g &&
        !g.failed &&
        current(g.plan.capture) &&
        presented !== g.version
      ) {
        const version = g.version,
          next = structuredClone(g.next),
          control = new AbortController();
        g.previewAbort = control;
        const live = () =>
          gesture.current === g &&
          g.version === version &&
          current(g.plan.capture);
        try {
          const c = g.plan.capture,
            transaction = await planTransforms(
              c.api,
              c,
              c.targets,
              next,
              g.draftId,
              control.signal,
              live,
              g.intent,
            );
          if (!live()) continue;
          await controller.v4.visualPreview.previewTransformDraft(
            {
              draftId: g.draftId,
              transaction,
              sequenceId: c.sequenceId,
              time: JSON.parse(c.timeKey),
              gestureIntent: g.intent,
            },
            control.signal,
          );
          if (live()) presented = version;
        } catch (cause) {
          if (control.signal.aborted || !live()) continue;
          g.failed = true;
          setError(cause instanceof Error ? cause.message : String(cause));
          cancelGesture();
        } finally {
          // A completed draft stays visible until its replacement is ready.
          // Only unfinished work needs aborting on another pointer move.
          if (g.previewAbort === control) g.previewAbort = null;
        }
      }
    } finally {
      g.rendering = false;
    }
  };
  const start = (event: ReactPointerEvent, handle: Handle) => {
    if (
      event.button !== 0 ||
      !plan ||
      pending ||
      !current(plan.capture) ||
      !scale
    )
      return;
    const state = controller.getSnapshot().v4;
    if (
      !state?.audioPreview.session ||
      state.audioPreview.phase !== "paused" ||
      controller.v4.visualPreview.snapshot().phase !== "ready"
    ) {
      setError("Pause the live preview before transforming picture content.");
      return;
    }
    const original = structuredClone(plan.transforms),
      intent = plan.v2 ? gestureIntent(handle, original.length > 1) : undefined,
      problem =
        (intent && gestureProblem(plan.capture.targets, intent)) ||
        (handle === "rotate"
          ? groupRotationProblem(members(plan, original))
          : null);
    if (problem) {
      setError(problem);
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    hitSerial.current++;
    event.currentTarget.setPointerCapture(event.pointerId);
    gesture.current = {
      plan,
      pointer: event.pointerId,
      node: event.currentTarget,
      handle,
      intent,
      from: point(event),
      original,
      next: structuredClone(original),
      draftId: crypto.randomUUID(),
      version: 0,
      rendering: false,
      previewAbort: null,
      failed: false,
    };
    draftOwner.current = gesture.current;
    setError(
      intent && clearsScaleAnimation(plan.capture.targets, intent)
        ? "Resizing removes animation on the resized scale axes. Other animation is preserved. Escape cancels without saving."
        : null,
    );
    setDraft(original);
  };
  const move = (event: ReactPointerEvent, preview = true) => {
    const g = gesture.current;
    if (!g || g.pointer !== event.pointerId) return;
    event.preventDefault();
    event.stopPropagation();
    if (!current(g.plan.capture)) {
      cancelGesture();
      return;
    }
    try {
      const next = dragSelection(
        members(g.plan, g.original),
        g.handle,
        g.from,
        point(event),
      );
      if (JSON.stringify(next) === JSON.stringify(g.next)) return;
      g.next = next;
      g.version++;
      g.previewAbort?.abort();
      setDraft(next);
      if (preview) void renderDraft(g);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
      cancelGesture();
    }
  };
  const save = async (g: Gesture) => {
    const c = g.plan.capture;
    g.previewAbort?.abort();
    if (
      g.failed ||
      !current(c) ||
      JSON.stringify(g.original) === JSON.stringify(g.next)
    ) {
      clearDraft(g);
      setDraft(null);
      return;
    }
    const planning = new AbortController();
    abort.current = planning;
    setPending(true);
    setError(null);
    try {
      const transaction = await planTransforms(
        c.api,
        c,
        c.targets,
        g.next,
        g.draftId,
        planning.signal,
        () => current(c),
        g.intent,
      );
      if (planning.signal.aborted || !current(c)) return;
      if (g.intent) {
        // Validate the exact final candidate using the same V2 admission as
        // preview. A quick pointer-up need not have produced an earlier frame.
        await controller.v4.visualPreview.previewTransformDraft(
          {
            draftId: g.draftId,
            transaction,
            sequenceId: c.sequenceId,
            time: JSON.parse(c.timeKey),
            gestureIntent: g.intent,
          },
          planning.signal,
        );
        if (planning.signal.aborted || !current(c)) return;
      }
      writing.current = true;
      await controller.v4.applyPlanned(transaction, c.sequenceId);
    } catch (cause) {
      if (!planning.signal.aborted && c.serial === serial.current)
        setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      planning.abort();
      writing.current = false;
      clearDraft(g);
      if (c.serial === serial.current) {
        setPending(false);
        setDraft(null);
      }
    }
  };
  const up = (event: ReactPointerEvent) => {
    if (gesture.current?.pointer !== event.pointerId) return;
    move(event, false);
    const g = gesture.current;
    gesture.current = null;
    if (!g) return;
    if (g.node.hasPointerCapture?.(g.pointer))
      g.node.releasePointerCapture(g.pointer);
    void save(g);
  };
  const hitContent = async (
    event: ReactPointerEvent | ReactMouseEvent,
    editText = false,
  ) => {
    if (!editText) {
      const press = hitPress.current;
      hitPress.current = null;
      if (
        !press ||
        !("pointerId" in event) ||
        press.pointer !== event.pointerId ||
        Math.hypot(event.clientX - press.x, event.clientY - press.y) > 3
      )
        return;
    }
    if (
      event.button !== 0 ||
      event.altKey ||
      editingDisabled ||
      writing.current ||
      blocked() ||
      !scale
    )
      return;
    event.preventDefault();
    event.stopPropagation();
    const at = point(event),
      pixel = { x: Math.floor(at.x), y: Math.floor(at.y) };
    if (pixel.x < 0 || pixel.y < 0 || pixel.x >= width || pixel.y >= height)
      return;
    const active = hitInFlight.current;
    if (active) {
      // A double click may arrive while its second pointer-up read is pending.
      // Only that exact pixel and still-live scope can promote the read to editing.
      if (
        editText &&
        active.pixel.x === pixel.x &&
        active.pixel.y === pixel.y &&
        active.current()
      )
        active.editText = true;
      return;
    }
    const initial = controller.getSnapshot(),
      view = initial.v4,
      sourceApi = initial.api,
      query = ++hitSerial.current;
    if (!view || !sourceApi) return;
    const connection = sourceApi.connectionIdentity,
      time = structuredClone(view.playhead),
      selection = selectionIdentity(view),
      modifier = event.metaKey || event.ctrlKey || event.shiftKey;
    const same = () => {
      const s = controller.getSnapshot(),
        v = s.v4;
      return (
        query === hitSerial.current &&
        !gesture.current &&
        !writing.current &&
        !blocked() &&
        s.api === sourceApi &&
        s.api.connectionIdentity === connection &&
        v?.head.project.id === view.head.project.id &&
        v.head.revision === view.head.revision &&
        v.sequenceId === view.sequenceId &&
        selectionIdentity(v) === selection &&
        JSON.stringify(v.playhead) === JSON.stringify(time)
      );
    };
    const observation = { pixel, editText, current: same };
    try {
      hitInFlight.current = observation;
      const result = await controller.v4.visualPreview.hitTest(pixel);
      if (!same()) return;
      if (
        result.projectId !== view.head.project.id ||
        result.revision !== view.head.revision ||
        result.sequenceId !== view.sequenceId ||
        BigInt(result.time.ticks) * BigInt(time.timebase) !==
          BigInt(time.ticks) * BigInt(result.time.timebase) ||
        new Set(result.hits.map((h) => h.elementId)).size !==
          result.hits.length ||
        result.hits.some(
          (h) => !Number.isInteger(h.alpha) || h.alpha < 1 || h.alpha > 255,
        )
      )
        throw Error("The pixel hit observation differs from the fixed canvas.");
      const sequence = view.head.project.sequences.find(
        (s) => s.id === view.sequenceId,
      );
      const target = result.hits.find((hit) =>
        sequence?.tracks.some(
          (track) =>
            track.enabled &&
            !track.locked &&
            track.elements.some(
              (element) =>
                element.id === hit.elementId &&
                element.enabled &&
                compareTime(time, element.timeline_range.start) >= 0 &&
                compareTime(
                  time,
                  addTime(
                    element.timeline_range.start,
                    element.timeline_range.duration,
                  ),
                ) < 0 &&
                !["audio", "effect_layer"].includes(element.content.kind),
            ),
        ),
      );
      if (same()) {
        setError(null);
        const element =
          target &&
          sequence?.tracks
            .flatMap((track) => track.elements)
            .find((element) => element.id === target.elementId);
        const authoring = latest.current.textAuthoring;
        const targetTrack =
          target &&
          sequence?.tracks.find((track) =>
            track.elements.some((e) => e.id === target.elementId),
          );
        if (modifier && !observation.editText) {
          if (target && targetTrack && sequence) {
            const selected = selectTimelineTarget(
              sequence,
              view.timelineEdit.selection,
              { track_id: targetTrack.id, element_id: target.elementId },
              event.shiftKey ? "extend" : "toggle",
            );
            controller.v4.timelineEdit.selectMany(
              selected,
              selected.at(-1)?.element_id ?? null,
            );
          }
        } else controller.v4.select(target?.elementId ?? null);
        if (
          observation.editText &&
          element?.content.kind === "text" &&
          authoring?.available &&
          !authoring.disabled &&
          !authoring.canvas
        )
          authoring.beginCanvasEdit(element.id);
      }
    } catch (cause) {
      if (same())
        setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      if (hitInFlight.current === observation) hitInFlight.current = null;
    }
  };
  const transforms = draft ?? plan?.transforms,
    outline =
      plan && transforms ? selectionOutline(members(plan, transforms)) : null;
  return (
    <div
      ref={container}
      className="v4-canvas-transform"
      onDoubleClick={(event) => void hitContent(event, true)}
    >
      {!editingDisabled && (
        <div
          className="v4-content-hit-surface"
          aria-label={t("Select visible picture content")}
          style={{
            left: fit.left,
            top: fit.top,
            width: fit.width,
            height: fit.height,
          }}
          onPointerDown={(event) => {
            if (event.button === 0)
              hitPress.current = {
                pointer: event.pointerId,
                x: event.clientX,
                y: event.clientY,
              };
          }}
          onPointerCancel={() => {
            hitPress.current = null;
          }}
          onPointerUp={(event) => void hitContent(event)}
        />
      )}
      {outline && plan && !blocked() && scale > 0 && (
        <svg
          aria-label={t("Selected element transform handles")}
          viewBox={`0 0 ${width} ${height}`}
          style={{
            left: fit.left,
            top: fit.top,
            width: fit.width,
            height: fit.height,
          }}
          onPointerMove={move}
          onPointerUp={up}
          onPointerCancel={cancelGesture}
          onLostPointerCapture={(event) => {
            if (gesture.current?.pointer === event.pointerId) cancelGesture();
          }}
        >
          <polygon
            points={outline.corners.map((p) => `${p.x},${p.y}`).join(" ")}
            fill="none"
            stroke="#ffd578"
            strokeWidth={1.5 / scale}
          />
          <polygon
            data-transform-handle="move"
            points={outline.corners.map((p) => `${p.x},${p.y}`).join(" ")}
            fill="none"
            stroke="transparent"
            strokeWidth={12 / scale}
            onPointerDown={(e) => start(e, "move")}
          />
          {outline.corners.map((p, index) => (
            <rect
              key={index}
              data-transform-handle="resize"
              aria-label={t("Resize selected element")}
              x={p.x - 4 / scale}
              y={p.y - 4 / scale}
              width={8 / scale}
              height={8 / scale}
              onPointerDown={(e) =>
                start(
                  e,
                  (
                    [
                      "top-left",
                      "top-right",
                      "bottom-right",
                      "bottom-left",
                    ] as const
                  )[index],
                )
              }
            />
          ))}
          {outline.corners.map((p, index) => {
            const next = outline.corners[(index + 1) % 4],
              handle = (["top", "right", "bottom", "left"] as const)[index];
            return (
              <rect
                key={`edge-${handle}`}
                data-transform-handle={handle}
                aria-label={t(
                  handle === "left" || handle === "right"
                    ? "Resize selected element horizontally"
                    : "Resize selected element vertically",
                )}
                x={(p.x + next.x) / 2 - 4 / scale}
                y={(p.y + next.y) / 2 - 4 / scale}
                width={8 / scale}
                height={8 / scale}
                onPointerDown={(e) => start(e, handle)}
              />
            );
          })}
          <circle
            data-transform-handle="move"
            aria-label={t("Move selected element")}
            cx={outline.center.x}
            cy={outline.center.y}
            r={5 / scale}
            onPointerDown={(e) => start(e, "move")}
          />
          <line
            x1={(outline.corners[0].x + outline.corners[1].x) / 2}
            y1={(outline.corners[0].y + outline.corners[1].y) / 2}
            x2={(outline.corners[0].x + outline.corners[1].x) / 2}
            y2={(outline.corners[0].y + outline.corners[1].y) / 2 - 28 / scale}
            stroke="#ffd578"
            strokeWidth={1 / scale}
          />
          <circle
            data-transform-handle="rotate"
            aria-label={t("Rotate selected element")}
            cx={(outline.corners[0].x + outline.corners[1].x) / 2}
            cy={(outline.corners[0].y + outline.corners[1].y) / 2 - 28 / scale}
            r={5 / scale}
            onPointerDown={(e) => start(e, "rotate")}
          />
        </svg>
      )}
      {error && (
        <p role="status" className="v4-canvas-transform-status">
          {t(error)}
        </p>
      )}
    </div>
  );
}
