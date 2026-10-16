import { useEffect, useRef, useState } from "react";
import type { PointerEvent as ReactPointerEvent } from "react";
import { useWorkspace } from "../../context";
import type { EditorApi } from "../../api";
import type { ToolInput, ToolOutput } from "../../../../lib/editor-sdk";
import type {
  KeyframeEdit,
  ScalarChannel,
  ScalarKey,
  ExactTime,
} from "../keyframes/types";
import { seconds } from "../keyframes/types";
import type { V4Element } from "../types";
import { fontRecoveryBlocked } from "../text/font-recovery";
import { audioGainAvailability } from "../audio-gain";
import {
  gainChannel,
  sampleTimes,
  axisBounds,
  dragKeys,
  boundedTimeDelta,
  localPlayhead,
} from "./model";
import { useGainText } from "./messages";
import "./gain-keyframes.css";
type Sample = ToolOutput<"opencut_keyframe_v1_sample">["samples"][number];
type Scope = {
  api: EditorApi;
  connection: string | null;
  project: string;
  revision: string;
  scene: string;
  trackId: string;
  element: V4Element;
  channel: ScalarChannel;
  timeKey: string;
  serial: number;
};
type Gesture = {
  scope: Scope;
  pointer: number;
  node: Element;
  from: { x: number; y: number };
  selected: Set<string>;
  whole: boolean;
  edits: KeyframeEdit[];
  rect: DOMRect;
};
export function V4GainKeyframeLine({
  elementId,
  onSelect,
  compact = false,
}: {
  elementId: string;
  compact?: boolean;
  onSelect?(elementId: string): void;
}) {
  const { api, v4, controller, busy } = useWorkspace(),
    t = useGainText(),
    surface = useRef<SVGSVGElement>(null);
  const [pixelWidth, setPixelWidth] = useState(300),
    [samples, setSamples] = useState<Sample[] | null>(null),
    [sampleScope, setSampleScope] = useState<Scope | null>(null),
    [selected, setSelected] = useState<Set<string>>(new Set()),
    [draft, setDraft] = useState<KeyframeEdit[] | null>(null),
    [pending, setPending] = useState(false),
    [error, setError] = useState<string | null>(null);
  const epoch = useRef(0),
    abort = useRef<AbortController | null>(null),
    drag = useRef<Gesture | null>(null),
    writing = useRef(false),
    measured = useRef({ width: 0, height: 0 });
  const available = v4 ? audioGainAvailability(v4, elementId) : null,
    element = available?.element,
    channel = element ? gainChannel(element) : null;
  const blocked = () => {
    const s = controller.getSnapshot(),
      v = s.v4;
    return (
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
      ["applying", "unconfirmed"].includes(v.clipboard.attempt?.phase ?? "") ||
      !!(v.audioGain && v.audioGain.phase !== "saved")
    );
  };
  const current = (scope: Scope) => {
    const s = controller.getSnapshot(),
      v = s.v4,
      a = v ? audioGainAvailability(v, elementId) : null;
    return (
      !blocked() &&
      epoch.current === scope.serial &&
      s.api === scope.api &&
      s.api.connectionIdentity === scope.connection &&
      v?.head.project.id === scope.project &&
      v.head.revision === scope.revision &&
      v.sequenceId === scope.scene &&
      JSON.stringify(v.playhead) === scope.timeKey &&
      !!a &&
      !a.reason &&
      JSON.stringify(a.element) === JSON.stringify(scope.element)
    );
  };
  const cancelDrag = () => {
    const g = drag.current;
    drag.current = null;
    if (g?.node.hasPointerCapture?.(g.pointer))
      g.node.releasePointerCapture(g.pointer);
    setDraft(null);
  };
  const invalidate = () => {
    epoch.current++;
    abort.current?.abort();
    cancelDrag();
    setSamples(null);
    setSampleScope(null);
    setSelected(new Set());
    if (!writing.current) setPending(false);
  };
  const timeKey = JSON.stringify(v4?.playhead);
  useEffect(() => {
    invalidate();
    setError(null);
    if (!api || !v4 || !element || !channel || available?.reason || blocked())
      return;
    const scope: Scope = {
      api,
      connection: api.connectionIdentity,
      project: v4.head.project.id,
      revision: v4.head.revision,
      scene: v4.sequenceId,
      trackId: available!.track.id,
      element: structuredClone(element),
      channel: structuredClone(channel),
      timeKey: JSON.stringify(v4.playhead),
      serial: epoch.current,
    };
    const local = localPlayhead(v4.playhead, element),
      times = sampleTimes(
        element.timeline_range.duration,
        channel.keys,
        seconds(local) >= 0 &&
          seconds(local) <= seconds(element.timeline_range.duration)
          ? local
          : undefined,
      );
    if (times.length > 1024) {
      setError("Volume automation has too many visible keys for this editor.");
      return;
    }
    const stop = new AbortController();
    abort.current = stop;
    setPending(true);
    void (async () => {
      try {
        const capabilities = await api.sdk.hostCapabilities();
        if (
          !capabilities.tools.includes("opencut_keyframe_v1_sample") ||
          !capabilities.tools.includes("opencut_keyframe_v1_plan")
        )
          throw Error("Automation editing is unavailable on this host.");
        const result = await api.sdk.call(
          "opencut_keyframe_v1_sample",
          {
            project_id: scope.project,
            revision: scope.revision,
            sequence_id: scope.scene,
            channel_id: channel.id,
            times,
          },
          { signal: stop.signal },
        );
        if (stop.signal.aborted || !current(scope)) return;
        if (
          result.schema_version !== 1 ||
          result.revision !== scope.revision ||
          result.channel_id !== channel.id ||
          result.samples.length !== times.length ||
          result.samples.some(
            (s, i) =>
              !Number.isFinite(s.value) ||
              !Number.isFinite(s.physical_value) ||
              s.time.ticks !== times[i].ticks ||
              s.time.timebase !== times[i].timebase,
          )
        )
          throw Error("The sampled gain curve differs from the fixed channel.");
        setSampleScope(scope);
        setSamples(result.samples);
      } catch (cause) {
        if (!stop.signal.aborted && scope.serial === epoch.current)
          setError(cause instanceof Error ? cause.message : String(cause));
      } finally {
        if (scope.serial === epoch.current) setPending(false);
      }
    })();
    return () => invalidate();
  }, [
    api,
    api?.connectionIdentity,
    v4?.head.project.id,
    v4?.head.revision,
    v4?.historical?.revision,
    v4?.sequenceId,
    timeKey,
    elementId,
    busy,
    v4?.audioPreview.phase,
    v4?.recovery,
    v4?.fontRecovery,
  ]);
  useEffect(() => {
    const node = surface.current;
    if (!node || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => {
      const r = node.getBoundingClientRect(),
        previous = measured.current;
      measured.current = { width: r.width, height: r.height };
      if (r.width > 0) setPixelWidth(r.width);
      if (
        previous.width &&
        (previous.width !== r.width || previous.height !== r.height)
      )
        cancelDrag();
    });
    observer.observe(node);
    return () => observer.disconnect();
  }, [!!samples]);
  useEffect(() => {
    const cancel = () => {
      if (!writing.current) {
        abort.current?.abort();
        cancelDrag();
        setPending(false);
      }
    };
    const key = (event: KeyboardEvent) => {
      if (
        event.key === "Escape" &&
        (drag.current || pending) &&
        !writing.current
      ) {
        event.preventDefault();
        event.stopPropagation();
        cancel();
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
  if (!available || !element || !channel || !v4) return null;
  const bounds = axisBounds(channel, samples ?? []),
    duration = seconds(element.timeline_range.duration),
    height = compact ? 32 : 76,
    width = pixelWidth;
  const x = (time: ExactTime) => (seconds(time) / duration) * width,
    y = (value: number) =>
      ((bounds.max - value) / (bounds.max - bounds.min)) * height;
  const draftMap = new Map(
      (draft ?? []).flatMap((edit) =>
        edit.kind === "put_scalar" ? [[edit.key.id, edit.key] as const] : [],
      ),
    ),
    keys = channel.keys.map((key) => draftMap.get(key.id) ?? key);
  const submit = async (scope: Scope, edits: KeyframeEdit[]) => {
    if (writing.current || !edits.length || !current(scope)) return;
    const request: ToolInput<"opencut_keyframe_v1_plan"> = {
      project_id: scope.project,
      expected_revision: scope.revision,
      idempotency_key: crypto.randomUUID(),
      label: "Edit volume automation",
      sequence_id: scope.scene,
      edits: structuredClone(edits),
    };
    const stop = new AbortController();
    abort.current = stop;
    setPending(true);
    setError(null);
    try {
      const tx = await scope.api.sdk.call("opencut_keyframe_v1_plan", request, {
        signal: stop.signal,
      });
      if (stop.signal.aborted || !current(scope)) return;
      if (
        tx.project_id !== request.project_id ||
        tx.expected_revision !== request.expected_revision ||
        tx.idempotency_key !== request.idempotency_key ||
        tx.label !== request.label ||
        tx.operations.length !== 1 ||
        tx.operations[0].type !== "element_put" ||
        tx.operations[0].element.id !== scope.element.id ||
        tx.operations[0].sequence_id !== scope.scene ||
        tx.operations[0].track_id !== scope.trackId
      )
        throw Error(
          "The gain-keyframe plan differs from the original gesture.",
        );
      writing.current = true;
      await controller.v4.applyPlanned(tx, scope.scene);
    } catch (cause) {
      if (!stop.signal.aborted && scope.serial === epoch.current)
        setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      writing.current = false;
      if (scope.serial === epoch.current) {
        setPending(false);
        setDraft(null);
      }
    }
  };
  const down = (event: ReactPointerEvent, key?: ScalarKey) => {
    event.stopPropagation();
    if (
      event.button !== 0 ||
      pending ||
      !sampleScope ||
      !current(sampleScope) ||
      drag.current
    )
      return;
    event.preventDefault();
    const group = key
      ? event.shiftKey
        ? new Set([...selected, key.id])
        : selected.has(key.id)
          ? new Set(selected)
          : new Set([key.id])
      : new Set(channel.keys.map((k) => k.id));
    setSelected(group);
    onSelect?.(elementId);
    const rect = surface.current!.getBoundingClientRect();
    if (!rect.width || !rect.height) return;
    event.currentTarget.setPointerCapture(event.pointerId);
    drag.current = {
      scope: sampleScope,
      pointer: event.pointerId,
      node: event.currentTarget,
      from: { x: event.clientX, y: event.clientY },
      selected: group,
      whole: !key,
      edits: [],
      rect,
    };
  };
  const move = (event: ReactPointerEvent) => {
    const g = drag.current;
    if (!g || g.pointer !== event.pointerId) return;
    event.preventDefault();
    event.stopPropagation();
    if (!current(g.scope)) {
      cancelDrag();
      return;
    }
    const timeDelta = g.whole
      ? { ticks: "0", timebase: 1 }
      : boundedTimeDelta(
          g.scope.channel.keys,
          g.selected,
          element.timeline_range.duration,
          ((event.clientX - g.from.x) / g.rect.width) * duration,
        );
    let valueDelta =
      (-(event.clientY - g.from.y) / g.rect.height) * (bounds.max - bounds.min);
    const values = g.scope.channel.keys
      .filter((k) => g.selected.has(k.id))
      .map((k) => k.value);
    valueDelta = Math.max(
      bounds.min - Math.min(...values),
      Math.min(bounds.max - Math.max(...values), valueDelta),
    );
    g.edits = dragKeys(g.scope.channel, g.selected, timeDelta, valueDelta);
    setDraft(g.edits);
  };
  const up = (event: ReactPointerEvent) => {
    if (drag.current?.pointer !== event.pointerId) return;
    move(event);
    const g = drag.current;
    drag.current = null;
    if (!g) return;
    if (g.node.hasPointerCapture?.(g.pointer))
      g.node.releasePointerCapture(g.pointer);
    const changed = g.edits.filter(
      (edit) =>
        edit.kind === "put_scalar" &&
        JSON.stringify(edit.key) !==
          JSON.stringify(
            g.scope.channel.keys.find((k) => k.id === edit.key.id),
          ),
    );
    void submit(g.scope, changed);
  };
  const addAtPlayhead = () => {
    if (!sampleScope || !samples || !current(sampleScope)) return;
    const time = localPlayhead(v4.playhead, element),
      sample = samples.find(
        (s) =>
          BigInt(s.time.ticks) * BigInt(time.timebase) ===
          BigInt(time.ticks) * BigInt(s.time.timebase),
      );
    if (
      !sample ||
      channel.keys.some(
        (k) =>
          BigInt(k.time.ticks) * BigInt(time.timebase) ===
          BigInt(time.ticks) * BigInt(k.time.timebase),
      )
    )
      return;
    const key: ScalarKey = {
      id: crypto.randomUUID(),
      time,
      value: sample.value,
      left_handle: null,
      right_handle: null,
      segment_to_next: { kind: "linear" },
      tangent_mode: "auto",
    };
    void submit(sampleScope, [
      { kind: "put_scalar", channel_id: channel.id, key },
    ]);
  };
  return (
    <section
      className={`v4-gain-keyframes${compact ? " v4-gain-keyframes-compact" : ""}`}
      data-timeline-no-gesture
      onPointerDown={(event) => event.stopPropagation()}
      aria-label={t("Volume automation")}
      onClick={(event) => event.stopPropagation()}
    >
      {samples && sampleScope && (
        <svg
          ref={surface}
          viewBox={`0 0 ${width} ${height}`}
          preserveAspectRatio="none"
          aria-label={t("Gain keyframes")}
          onPointerMove={move}
          onPointerUp={up}
          onPointerCancel={cancelDrag}
          onLostPointerCapture={cancelDrag}
        >
          <polyline
            className="gain-sampled-curve"
            points={samples
              .map((s) => `${x(s.time)},${y(s.physical_value)}`)
              .join(" ")}
            fill="none"
            vectorEffect="non-scaling-stroke"
          />
          <polyline
            role="button"
            aria-label={t("Move volume automation")}
            data-gain-handle="curve"
            points={samples
              .map((s) => `${x(s.time)},${y(s.physical_value)}`)
              .join(" ")}
            fill="none"
            stroke="transparent"
            strokeWidth="12"
            vectorEffect="non-scaling-stroke"
            onPointerDown={(event) => down(event)}
          />
          {keys
            .filter(
              (key) => seconds(key.time) >= 0 && seconds(key.time) <= duration,
            )
            .map((key) => (
              <circle
                key={key.id}
                role="button"
                aria-label={`${t("Gain keyframe")} ${key.id}`}
                aria-pressed={selected.has(key.id)}
                data-key-id={key.id}
                data-gain-handle="key"
                cx={x(key.time)}
                cy={y(key.value)}
                r={5}
                vectorEffect="non-scaling-stroke"
                className={selected.has(key.id) ? "selected" : ""}
                onPointerDown={(event) => down(event, key)}
              >
                <title>
                  {seconds(key.time).toFixed(3)}s · {key.value.toFixed(2)}{" "}
                  {bounds.unit}
                </title>
              </circle>
            ))}
        </svg>
      )}
      <p>
        {t(
          pending
            ? writing.current
              ? "Saving volume keys…"
              : "Sampling volume curve…"
            : draft
              ? "Draft keys; the curve still shows the saved revision."
              : "Drag a key to change time and value. Shift-click selects more keys; drag the curve to offset all keys.",
        )}
      </p>
      <div className="gain-keyframe-actions">
        <button
          type="button"
          disabled={pending || blocked() || !sampleScope}
          onClick={addAtPlayhead}
        >
          {t("Add key at playhead")}
        </button>
        <button
          type="button"
          disabled={pending || blocked() || !sampleScope || !selected.size}
          onClick={() => {
            if (sampleScope)
              void submit(sampleScope, [
                {
                  kind: "delete",
                  keys: [...selected].map((id) => ({
                    channel_id: channel.id,
                    key_id: id,
                  })),
                },
              ]);
          }}
        >
          {t("Delete selected keys")}
        </button>
        {draft && (
          <button type="button" onClick={cancelDrag}>
            {t("Cancel")}
          </button>
        )}
      </div>
      {error && <p role="alert">{t(error)}</p>}
    </section>
  );
}
