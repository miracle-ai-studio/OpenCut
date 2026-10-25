import { useResourceDrag } from "./resource-drag";
import { useEffect, useState } from "react";
import { Layers, Play, Square, Plus, RefreshCw, Trash2, X } from "lucide-react";
import { Button, Field, Modal } from "../../../shared/ui";
import { ErrorNotice } from "../../../shared/error-notice";
import { t } from "../../../shared/locale";
import { secondsForDisplay, timeFromDecimalSeconds } from "../../../model/time";
import { ResourceIllustration } from "./effects";
import { transitionPresets } from "./resource-presets";
import { useResourceCopy } from "./resources-copy";
import { useTransitionCopy } from "./transition-copy";
import {
  parseTransitionDuration,
  useTransitions,
} from "./transition-workspace";
import { useWorkbenchView } from "./view";
import "./resources.css";
import { usePlayerContext } from "./player-context";

/** Resources preview independently of the explicit timeline cut selection. */
export function TransitionResources({ active = true }: { active?: boolean }) {
  const resourceDrag = useResourceDrag();
  const player = usePlayerContext();
  const c = useResourceCopy(),
    copy = useTransitionCopy();
  const state = useTransitions(),
    view = useWorkbenchView();
  const [preview, setPreview] = useState<
    (typeof transitionPresets)[number] | null
  >(null);
  useEffect(() => {
    if (!active) setPreview(null);
  }, [active]);
  const apply = (seconds: string) => {
    view.showProperties();
    state?.apply(timeFromDecimalSeconds(seconds));
  };
  const action = state?.existing ? copy("replace") : t("Add transition");
  return (
    <section
      className="workbench-transition-resources"
      aria-label={t("Scene transitions")}
    >
      <div className="workbench-resource-title">
        <Layers size={17} />
        <h3>{t("Crossfade")}</h3>
      </div>
      <p className="transition-target-hint" role="status">
        {state?.pair
          ? `${copy("target")} · ${state.pair.outgoing.name} → ${state.pair.incoming.name}`
          : copy("choose")}
      </p>
      {state?.existing && (
        <p className="transition-replace-note">{copy("replacement")}</p>
      )}
      <div
        className="resource-preset-grid transition-resource-grid"
        aria-label={c("presets")}
      >
        {transitionPresets.map((preset) => {
          const reason =
            state?.durationReason(timeFromDecimalSeconds(preset.seconds)) ??
            (!state ? copy("choose") : null);
          const label = `${action} · ${c(preset.name)}`;
          return (
            <div
              className="resource-preset-tile resource-transition-card"
              key={preset.seconds}
            >
              <button
                type="button"
                className="resource-preset-card"
                draggable={!!resourceDrag && !resourceDrag.disabled}
                onDragStart={event => resourceDrag?.start(event, {kind:"transition",name:c(preset.name),duration:timeFromDecimalSeconds(preset.seconds)})}
                onDragEnd={() => resourceDrag?.cancel()}
                aria-label={`${copy("preview")} · ${c(preset.name)}`}
                onClick={() => player ? void player.open({ kind: "transition", name: c(preset.name), seconds: preset.seconds }) : setPreview(preset)}
                onKeyDown={(event) => {
                  if (event.key === "Enter" && event.shiftKey) {
                    event.preventDefault();
                    event.stopPropagation();
                    if (!event.repeat && !reason && !state?.pending)
                      apply(preset.seconds);
                  }
                }}
              >
                <div className="resource-example">
                  <ResourceIllustration />
                  <span className="transition-sample-badge">
                    <Layers size={18} />
                  </span>
                </div>
                <strong title={c(preset.name)}>{c(preset.name)}</strong>
              </button>
              <Button
                className="resource-transition-apply"
                icon={
                  state?.existing ? <RefreshCw size={13} /> : <Plus size={13} />
                }
                disabled={!!reason || state?.pending}
                aria-label={label}
                title={reason ?? label}
                onClick={() => apply(preset.seconds)}
              >
                {state?.existing ? copy("replace") : t("Apply")}
              </Button>
            </div>
          );
        })}
      </div>
      {state?.disabledReason && state.pair && (
        <p role="status">{state.disabledReason}</p>
      )}
      {active && preview && (
        <Modal
          title={`${copy("preview")} · ${c(preview.name)}`}
          onClose={() => setPreview(null)}
          className="transition-preview-dialog"
        >
          <TransitionExample seconds={preview.seconds} />
          <p className="muted">{c("sample")}</p>
        </Modal>
      )}
    </section>
  );
}

export function TransitionExample({ seconds }: { seconds: string }) {
  const c = useResourceCopy();
  const [playing, setPlaying] = useState(false),
    [blend, setBlend] = useState(0);
  useEffect(() => {
    if (!playing) return;
    let frame = 0,
      start: number | null = null;
    const duration = Number(seconds) * 1000;
    const animate = (now: number) => {
      start ??= now;
      const elapsed = now - start;
      setBlend(Math.min(1, Math.max(0, (elapsed - 400) / duration)));
      if (elapsed < duration + 900) frame = requestAnimationFrame(animate);
      else setPlaying(false);
    };
    frame = requestAnimationFrame(animate);
    return () => cancelAnimationFrame(frame);
  }, [playing, seconds]);
  return (
    <>
      <div className="resource-example" aria-hidden="true">
        <ResourceIllustration />
        <div className="resource-example-layer" style={{ opacity: blend }}>
          <ResourceIllustration alternate />
        </div>
      </div>
      <Button
        icon={playing ? <Square size={13} /> : <Play size={13} />}
        onClick={() => {
          setBlend(0);
          setPlaying(!playing);
        }}
      >
        {c(playing ? "stopExample" : "playExample")}
      </Button>
    </>
  );
}

export function TransitionInspector() {
  const state = useTransitions();
  if (!state?.pair) return null;
  return (
    <TransitionProperties
      key={JSON.stringify([state.target, state.existing, state.revision])}
    />
  );
}
function TransitionProperties() {
  const state = useTransitions()!,
    copy = useTransitionCopy();
  const initial = state.existing
    ? String(secondsForDisplay(state.existing.duration))
    : "0.5";
  const [draft, setDraft] = useState(initial);
  // An untouched rational duration is passed through without decimal rounding.
  const duration =
    state.existing && draft === initial
      ? state.existing.duration
      : parseTransitionDuration(draft);
  const reason = state.durationReason(duration),
    pair = state.pair!;
  const apply = () => {
    if (duration && !reason) state.apply(duration);
  };
  return (
    <section className="transition-inspector" aria-label={copy("target")}>
      <div className="transition-inspector-heading">
        <h3>{t("Crossfade")}</h3>
        <Button
          icon={<X size={14} />}
          aria-label={copy("close")}
          onClick={() => state.select(null)}
        />
      </div>
      <dl className="transition-target-summary">
        <dt>{t("Track")}</dt>
        <dd title={pair.track.name}>{pair.track.name}</dd>
        <dt>{copy("target")}</dt>
        <dd>
          <span title={pair.outgoing.name}>{pair.outgoing.name}</span>
          <span aria-hidden="true">↓</span>
          <span title={pair.incoming.name}>{pair.incoming.name}</span>
        </dd>
      </dl>
      {state.existing && (
        <p className="transition-replace-note">{copy("replacement")}</p>
      )}
      <Field label={t("Duration (seconds)")}>
        <input
          inputMode="decimal"
          value={draft}
          disabled={state.pending || !!state.disabledReason}
          aria-invalid={!!reason}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              event.stopPropagation();
              if (!event.repeat) apply();
            }
            if (event.key === "Escape") {
              event.preventDefault();
              event.stopPropagation();
              setDraft(initial);
            }
          }}
        />
      </Field>
      {state.limit?.maximum && (
        <p className="transition-duration-limit">
          {copy("maximum")} ·{" "}
          {secondsForDisplay(state.limit.maximum).toLocaleString(undefined, {
            maximumFractionDigits: 6,
          })}{" "}
          {t("seconds")}
        </p>
      )}
      <p className="muted">{copy("sourceNote")}</p>
      {reason && <p role="status">{reason}</p>}
      <div className="transition-inspector-actions">
        <Button
          className="primary"
          disabled={state.pending || !!reason}
          icon={state.existing ? <RefreshCw size={14} /> : <Plus size={14} />}
          onClick={apply}
        >
          {state.existing ? copy("replace") : t("Add transition")}
        </Button>
        {state.existing && (
          <Button
            icon={<Trash2 size={14} />}
            aria-label={t("Remove transition")}
            title={t("Remove transition")}
            disabled={state.removeDisabled}
            onClick={state.remove}
          />
        )}
      </div>
      {state.pending && <p role="status">{copy("wait")}</p>}
      {state.notice && <p role="status">{state.notice}</p>}
      {state.error && <ErrorNotice error={state.error} />}
    </section>
  );
}
