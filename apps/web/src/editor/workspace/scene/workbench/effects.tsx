import { useResourceDrag } from "./resource-drag";
import { ErrorNotice } from "../../../shared/error-notice";
import { useEffect, useId, useRef, useState } from "react";
import { Sparkles, Plus } from "lucide-react";
import { Button, Field, Modal } from "../../../shared/ui";
import { timeFromDecimalSeconds } from "../../../model/time";
import { useWorkspace } from "../../context";
import { SceneEffectsPanel, useSceneStack } from "../scene-stack";
import { newEffectTrack, stackTargetProblem } from "../scene-stack/model";
import type { Sequence } from "../types";
import { blurPresets, presetBlur } from "./resource-presets";
import { useResourceCopy } from "./resources-copy";
import "./resources.css";
import { usePlayerContext } from "./player-context";

/** An illustrative resource sample, never a rendering of the user's document. */
export function ResourceIllustration({
  alternate = false,
  blur = 0,
}: {
  alternate?: boolean;
  blur?: number;
}) {
  const id = useId();
  return (
    <svg
      viewBox="0 0 192 108"
      preserveAspectRatio="xMidYMid slice"
      aria-hidden="true"
    >
      <defs>
        <filter id={id}>
          <feGaussianBlur stdDeviation={blur} />
        </filter>
      </defs>
      <g filter={blur ? `url(#${id})` : undefined}>
        <rect
          width="192"
          height="108"
          fill={alternate ? "#a7633a" : "#3b7296"}
        />
        <circle
          cx={alternate ? 138 : 48}
          cy="32"
          r="17"
          fill={alternate ? "#f4d6a3" : "#d5e9ee"}
        />
        <path
          d="M-10 110 54 47 105 110Z"
          fill={alternate ? "#d18e65" : "#81b7bd"}
        />
        <path
          d="m57 110 73-79 74 79Z"
          fill={alternate ? "#754932" : "#25485e"}
        />
      </g>
    </svg>
  );
}
export function EffectsResources({
  sequence,
  active = true,
}: {
  sequence: Sequence;
  active?: boolean;
}) {
  const resourceDrag = useResourceDrag();
  const player = usePlayerContext();
  const c = useResourceCopy(),
    { scene } = useWorkspace(),
    stack = useSceneStack();
  const [intensity, setIntensity] = useState(15),
    [duration, setDuration] = useState("5"),
    [saved, setSaved] = useState(false),
    [pending, setPending] = useState(false),
    [preview, setPreview] = useState<(typeof blurPresets)[number] | null>(null);
  const writing = useRef(false);
  const context = JSON.stringify([
    scene?.head.project.id,
    sequence.id,
    scene?.selectedId,
  ]);
  const currentContext = useRef(context);
  currentContext.current = context;
  useEffect(() => {
    setSaved(false);
  }, [context]);
  useEffect(() => {
    if (!active) setPreview(null);
  }, [active]);
  const targetTrack = sequence.tracks.find((track) =>
    track.elements.some((element) => element.id === scene?.selectedId),
  );
  const target = targetTrack?.elements.find(
    (element) => element.id === scene?.selectedId,
  );
  const clipAllowed =
    !!target?.visual && !targetTrack?.locked && !stackTargetProblem(target);
  const disabled = !stack || stack.disabled || !stack.available || pending;
  const add = async (layer: boolean, requestedIntensity = intensity) => {
    if (
      !scene ||
      !stack ||
      disabled ||
      writing.current ||
      (!layer && !clipAllowed)
    )
      return;
    const startedContext = context;
    writing.current = true;
    setPending(true);
    setSaved(false);
    try {
      let confirmed = false;
      if (layer) {
        const time = timeFromDecimalSeconds(duration);
        if (BigInt(time.ticks) <= 0n) throw Error(c("positiveDuration"));
        const track = newEffectTrack(scene.playhead, time);
        track.name = c("blur");
        track.elements[0].name = c("blur");
        if (track.elements[0].content.kind === "effect_layer")
          track.elements[0].content.effect = presetBlur(requestedIntensity);
        confirmed = await stack.transact("Add effect layer", [
          { type: "track_put", sequence_id: sequence.id, track },
        ]);
      } else if (target && targetTrack && clipAllowed) {
        const element = structuredClone(target);
        element.visual!.effects.push(presetBlur(requestedIntensity));
        confirmed = await stack.transact("Add blur", [
          {
            type: "element_put",
            sequence_id: sequence.id,
            track_id: targetTrack.id,
            element,
          },
        ]);
      }
      if (currentContext.current === startedContext) setSaved(confirmed);
    } catch (error) {
      stack.report(error);
    } finally {
      writing.current = false;
      setPending(false);
    }
  };
  return (
    <section aria-label={c("tabEffect")}>
      <div className="workbench-resource-title">
        <Sparkles size={17} />
        <h3>{c("blur")}</h3>
      </div>
      <div className="resource-preset-grid" aria-label={c("presets")}>
        {blurPresets.map((preset) => (
          <div className="resource-preset-tile" key={preset.intensity}>
            <button
              type="button"
              className="resource-preset-card"
              draggable={!!resourceDrag && !disabled && !resourceDrag.disabled}
              onDragStart={(event) => {
                try {
                  const span = timeFromDecimalSeconds(duration);
                  if (BigInt(span.ticks) <= 0n)
                    throw Error(c("positiveDuration"));
                  resourceDrag?.start(event, {
                    kind: "effect",
                    name: c(preset.name),
                    intensity: preset.intensity,
                    duration: span,
                  });
                } catch (error) {
                  event.preventDefault();
                  stack?.report(error);
                }
              }}
              onDragEnd={() => resourceDrag?.cancel()}
              key={preset.intensity}
              aria-pressed={intensity === preset.intensity}
              aria-label={`${c("preview")} · ${c(preset.name)}`}
              aria-keyshortcuts="Enter Shift+Enter"
              title={c("effectKeyboard")}
              onClick={() => {
                setIntensity(preset.intensity);
                setSaved(false);
                if (player) void player.open({ kind: "effect", name: c(preset.name), intensity: preset.intensity });
                else setPreview(preset);
              }}
              onKeyDown={(event) => {
                if (event.key === "Enter" && event.shiftKey) {
                  event.preventDefault();
                  event.stopPropagation();
                  if (!event.repeat) void add(false, preset.intensity);
                }
              }}
            >
              <div className="resource-example">
                <ResourceIllustration blur={preset.intensity / 8} />
              </div>
              <strong title={c(preset.name)}>{c(preset.name)}</strong>
            </button>
            <Button
              className="resource-preset-apply"
              aria-label={`${c("applyClip")} · ${c(preset.name)}`}
              title={
                clipAllowed
                  ? `${c("applyClip")} · ${target!.name}`
                  : c("chooseClip")
              }
              disabled={disabled || !clipAllowed}
              onClick={() => void add(false, preset.intensity)}
              icon={<Plus size={14} />}
            />
          </div>
        ))}
      </div>
      <p className="resource-example-note">{c("sample")}</p>
      <p className="muted">{clipAllowed ? target!.name : c("chooseClip")}</p>
      <details className="resource-details">
        <summary>{c("applyLayer")}</summary>
        <p className="muted">{c("effectLayerHint")}</p>
        <Field label={c("layerDuration")}>
          <input
            inputMode="decimal"
            value={duration}
            disabled={disabled}
            onChange={(e) => setDuration(e.target.value)}
          />
        </Field>
        <Button disabled={disabled} onClick={() => void add(true)}>
          {c("applyLayer")}
        </Button>
      </details>
      {saved && <p role="status">{c("effectSaved")}</p>}
      {stack?.error && <ErrorNotice error={stack.error} />}
      {!!sequence.tracks.some((track) => track.kind === "effect") && (
        <details className="resource-details">
          <summary>{c("appliedEffects")}</summary>
          <SceneEffectsPanel />
        </details>
      )}
      {active && preview && (
        <Modal
          title={`${c("preview")} · ${c(preview.name)}`}
          onClose={() => setPreview(null)}
          className="effect-preview-dialog"
        >
          <EffectExample intensity={preview.intensity} name={c(preview.name)} />
          <p className="muted">{c("sample")}</p>
        </Modal>
      )}
    </section>
  );
}

export function EffectExample({ intensity, name }: { intensity: number; name: string }) {
  const c = useResourceCopy();
  return <div className="effect-preview-compare">
    <figure><div className="resource-example"><ResourceIllustration /></div><figcaption>{c("original")}</figcaption></figure>
    <figure><div className="resource-example"><ResourceIllustration blur={intensity / 8} /></div><figcaption>{c("effectExample")} · {name}</figcaption></figure>
  </div>;
}
