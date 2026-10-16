import { useEffect, useRef, useState } from "react";
import { Button, Field, Modal } from "../../../shared/ui";
import { t, useLocale } from "../../../shared/locale";
import {
  formatTime,
  secondsForDisplay,
  timeFromDecimalSeconds,
} from "../../../model/time";
import type { Time } from "../../../model/types";
import type { V4Sequence } from "../types";
import type { V4TimelineEdit, TrackRemovalRequest, TrackRemovalReview } from "./controller";
import type {
  TimelineAttempt,
  TimelineCommand,
  TimelineScope,
  TimelineTarget,
} from "./types";
import { timelineSelectionReason } from "./selection";

type Action = "move" | "trim" | "split" | "delete" | "add_track";
type Props = {
  controller: V4TimelineEdit;
  sequence: V4Sequence;
  scope: TimelineScope;
  playhead: Time;
  selection: TimelineTarget[];
  attempt?: TimelineAttempt | null;
  selectedTrackId?: string | null;
  disabled: boolean;
  onReview(): void;
};

/** Presentation and explicit commands only. Planner owns alignment, source
 * bounds, local curves, overlap and ripple; forms never edit an element copy. */
export function TimelineEditControls(props: Props) {
  useLocale();
  const [form, setForm] = useState<{
    action: Action;
    scope: TimelineScope;
    targets: TimelineTarget[];
  } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [removal, setRemoval] = useState<{ request: TrackRemovalRequest; identity: ReturnType<V4TimelineEdit["identity"]> } | null>(null);
  const reason = timelineSelectionReason(props.sequence, props.selection);
  const selectedTrack = props.sequence.tracks.find(
    (track) => track.id === props.selectedTrackId,
  );
  const open = (action: Action) => {
    setError(null);
    setForm({
      action,
      scope: { ...props.scope },
      targets: structuredClone(props.selection),
    });
  };
  const planTrack = async (command: TimelineCommand) => {
    setError(null);
    try {
      await props.controller.plan(command, "Edit track", props.scope);
      props.onReview();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      props.onReview();
    }
  };
  return (
    <>
      <div
        className="toolbar v4-timeline-edit-toolbar"
        aria-label={t("Timeline editing")}
      >
        <span>{t("{count} selected", { count: props.selection.length })}</span>
        <Button
          disabled={props.disabled || !!reason}
          onClick={() => open("move")}
        >
          {t("Move selection")}
        </Button>
        <Button
          disabled={props.disabled || !!reason || props.selection.length !== 1}
          onClick={() => open("trim")}
        >
          {t("Trim element")}
        </Button>
        <Button
          disabled={props.disabled || !!reason}
          onClick={() => open("split")}
        >
          {t("Split selection")}
        </Button>
        <Button
          disabled={props.disabled || !!reason}
          onClick={() => open("delete")}
        >
          {t("Delete selection")}
        </Button>
        <Button disabled={props.disabled} onClick={() => open("add_track")}>
          {t("Add track")}
        </Button>
        {props.attempt && (
          <Button onClick={props.onReview}>
            {t("Review timeline changes")}
          </Button>
        )}
        {selectedTrack && (
          <>
            <Button
              disabled={props.disabled}
              onClick={() =>
                void planTrack({
                  kind: "set_track_locked",
                  track_id: selectedTrack.id,
                  locked: !selectedTrack.locked,
                })
              }
            >
              {t(selectedTrack.locked ? "Unlock track" : "Lock track")}
            </Button>
            <Button
              disabled={props.disabled || selectedTrack.locked}
              onClick={() =>
                void planTrack({
                  kind: "set_track_enabled",
                  track_id: selectedTrack.id,
                  enabled: !selectedTrack.enabled,
                })
              }
            >
              {t(selectedTrack.enabled ? "Disable track" : "Enable track")}
            </Button>
            <Button
              disabled={props.disabled || selectedTrack.locked}
              onClick={() =>
                void planTrack({
                  kind: "set_track_muted",
                  track_id: selectedTrack.id,
                  muted: !selectedTrack.muted,
                })
              }
            >
              {t(selectedTrack.muted ? "Unmute track" : "Mute track")}
            </Button>
            <Button
              disabled={
                props.disabled ||
                selectedTrack.locked ||
                selectedTrack.id === props.sequence.main_track_id ||
                selectedTrack.elements.length > 0 ||
                props.sequence.captions.some(
                  (caption) => caption.track_id === selectedTrack.id,
                )
              }
              onClick={() =>
                void planTrack({
                  kind: "delete_empty_track",
                  track_id: selectedTrack.id,
                })
              }
            >
              {t("Delete empty track")}
            </Button>
            <Button
              disabled={props.disabled || selectedTrack.locked || selectedTrack.id === props.sequence.main_track_id}
              onClick={() => setRemoval({
                request: { project_id: props.scope.projectId, expected_revision: props.scope.revision,
                  sequence_id: props.scope.sequenceId, track_id: selectedTrack.id,
                  idempotency_key: crypto.randomUUID(), label: "Remove track" },
                identity: props.controller.identity(),
              })}
            >{t("Delete track and contents")}</Button>
          </>
        )}
      </div>
      {reason && <p className="muted">{t(reason)}</p>}
      {error && <p role="alert">{t(error)}</p>}
      {removal && <TrackRemovalDialog controller={props.controller} original={removal}
        scope={props.scope} disabled={props.disabled} onClose={() => setRemoval(null)} />}
      {form && (
        <TimelineCommandForm
          {...props}
          action={form.action}
          capturedScope={form.scope}
          targets={form.targets}
          onClose={() => setForm(null)}
          onPlanned={() => {
            setForm(null);
            props.onReview();
          }}
        />
      )}
    </>
  );
}

function TrackRemovalDialog({ controller, original, scope, disabled, onClose }: {
  controller: V4TimelineEdit;
  original: { request: TrackRemovalRequest; identity: ReturnType<V4TimelineEdit["identity"]> };
  scope: TimelineScope;
  disabled: boolean;
  onClose(): void;
}) {
  const [review, setReview] = useState<TrackRemovalReview | null>(null),
    [pending, setPending] = useState(false), [submitted, setSubmitted] = useState(false),
    [error, setError] = useState<string | null>(null);
  const alive = useRef(true), inFlight = useRef(false), read = useRef<AbortController | null>(null);
  useEffect(() => {
    alive.current = true;
    return () => { alive.current = false; read.current?.abort(); };
  }, []);
  const current = controller.identity(), stale = original.identity.api !== current.api ||
    original.identity.generation !== current.generation || original.identity.viewGeneration !== current.viewGeneration ||
    scope.projectId !== original.request.project_id || scope.sequenceId !== original.request.sequence_id ||
    (!submitted && scope.revision !== original.request.expected_revision);
  const close = () => {
    if (pending && submitted) return;
    read.current?.abort();
    onClose();
  };
  const prepare = async () => {
    if (inFlight.current || stale || disabled) return;
    const cancel = new AbortController(); read.current = cancel; inFlight.current = true; setPending(true); setError(null);
    try {
      const result = await controller.prepareTrackRemoval(original.request, original.identity, cancel.signal);
      if (alive.current && !cancel.signal.aborted) setReview(result);
    } catch (cause) {
      if (alive.current && !cancel.signal.aborted) setError(cause instanceof Error ? cause.message : String(cause));
    } finally { inFlight.current = false; if (alive.current) setPending(false); }
  };
  const apply = async () => {
    if (!review || inFlight.current || stale || (!submitted && disabled)) return;
    inFlight.current = true; setPending(true); setSubmitted(true); setError(null);
    try { await controller.applyTrackRemoval(review); if (alive.current) onClose(); }
    catch (cause) { if (alive.current) setError(cause instanceof Error ? cause.message : String(cause)); }
    finally { inFlight.current = false; if (alive.current) setPending(false); }
  };
  return <Modal title={t("Review track removal")} onClose={close}>
    <p>{t("The selected track and its contents will be removed together. Media and fonts remain in the project library.")}</p>
    {review && <p>{review.trackName} · {t("{count} elements", { count: review.elements })} · {t("{count} captions", { count: review.captions })} · {t("{count} transitions", { count: review.transitions })}</p>}
    {stale && <p role="alert">{t("The track removal target changed. Return to its original project and review again.")}</p>}
    {error && <p role="alert">{t(error)}</p>}
    {submitted && error && <p>{t("Retry sends the same transaction and key. The shared recovery panel retains an uncertain edit across refresh.")}</p>}
    {!review ? <Button disabled={pending || disabled || stale} onClick={() => void prepare()}>{t(error ? "Retry original track plan" : "Review track contents")}</Button>
      : <Button disabled={pending || stale || (!submitted && disabled)} onClick={() => void apply()}>{t(submitted ? "Retry original track removal" : "Confirm track removal")}</Button>}
    <Button disabled={pending && submitted} onClick={close}>{t("Cancel")}</Button>
  </Modal>;
}

function TimelineCommandForm({
  action,
  targets,
  capturedScope,
  onClose,
  onPlanned,
  ...props
}: Props & {
  action: Action;
  targets: TimelineTarget[];
  capturedScope: TimelineScope;
  onClose(): void;
  onPlanned(): void;
}) {
  const titles = {
    move: "Move selection",
    trim: "Trim element",
    split: "Split selection",
    delete: "Delete selection",
    add_track: "Add track",
  };
  const [rows, setRows] = useState(() =>
    targets.map((target) => {
      const element = props.sequence.tracks
        .find((track) => track.id === target.track_id)
        ?.elements.find((e) => e.id === target.element_id);
      const caption = props.sequence.captions.find(
        (c) => c.id === target.element_id && c.track_id === target.track_id,
      );
      const range = element?.timeline_range ?? caption?.timeline_range;
      return {
        target,
        name: element?.name ?? caption?.text ?? target.element_id,
        kind: element?.content.kind ?? (caption ? "caption" : undefined),
        targetTrackId: target.track_id,
        initial: range?.start ?? props.playhead,
        start: secondsForDisplay(range?.start ?? props.playhead).toFixed(6),
        edited: false,
        rightId: crypto.randomUUID(),
      };
    }),
  );
  const [initialTime] = useState(props.playhead);
  const [time, setTime] = useState(secondsForDisplay(initialTime).toFixed(6)),
    [timeEdited, setTimeEdited] = useState(false);
  const [edge, setEdge] = useState<"left" | "right">("left");
  const [retain, setRetain] = useState<"both" | "left" | "right">("both");
  const [ripple, setRipple] = useState(false);
  const [trackId] = useState(() => crypto.randomUUID());
  const [trackName, setTrackName] = useState(t("New track"));
  const [trackKind, setTrackKind] = useState<
    "video" | "audio" | "graphic" | "text" | "effect" | "caption"
  >("video");
  const [pending, setPending] = useState(false),
    [error, setError] = useState<string | null>(null);
  const stale =
    capturedScope.projectId !== props.scope.projectId ||
    capturedScope.revision !== props.scope.revision ||
    capturedScope.sequenceId !== props.scope.sequenceId;
  const close = () => {
    if (pending && props.attempt?.phase === "planning")
      props.controller.cancel();
    onClose();
  };
  const submit = async () => {
    setPending(true);
    setError(null);
    try {
      const at = () =>
        timeEdited ? timeFromDecimalSeconds(time) : initialTime;
      const command: TimelineCommand =
        action === "move"
          ? {
              kind: "move_selection",
              ripple,
              moves: rows.map((row) => ({
                target: row.target,
                target_track_id: row.targetTrackId,
                new_start: row.edited
                  ? timeFromDecimalSeconds(row.start)
                  : row.initial,
              })),
            }
          : action === "trim"
            ? { kind: "trim", target: targets[0], edge, boundary: at(), ripple }
            : action === "split"
              ? {
                  kind: "split",
                  targets: rows.map((row) => ({
                    target: row.target,
                    right_element_id: row.rightId,
                  })),
                  at: at(),
                  retain,
                  ripple,
                }
              : action === "delete"
                ? { kind: "delete", targets, ripple }
                : {
                    kind: "add_track",
                    id: trackId,
                    name: trackName,
                    track_kind: trackKind,
                  };
      await props.controller.plan(command, titles[action], capturedScope);
      onPlanned();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setPending(false);
    }
  };
  return (
    <Modal title={t(titles[action])} onClose={close}>
      <div className="padded scroll">
        {action === "move" &&
          rows.map((row, index) => (
            <fieldset key={row.target.element_id}>
              <legend>{row.name}</legend>
              <Field label={t("Target track for {name}", { name: row.name })}>
                <select
                  value={row.targetTrackId}
                  onChange={(e) =>
                    setRows((current) =>
                      current.map((item, i) =>
                        i === index
                          ? { ...item, targetTrackId: e.target.value }
                          : item,
                      ),
                    )
                  }
                >
                  {props.sequence.tracks
                    .filter((track) =>
                      row.kind === "audio"
                        ? track.kind === "audio"
                        : row.kind === "video"
                          ? track.kind === "video"
                          : row.kind === "text"
                            ? track.kind === "text"
                            : row.kind === "graphic"
                              ? track.kind === "graphic"
                              : row.kind === "effect_layer"
                                ? track.kind === "effect"
                                : row.kind === "caption"
                                  ? track.kind === "caption"
                                  : ["video", "graphic"].includes(track.kind),
                    )
                    .map((track) => (
                      <option
                        key={track.id}
                        value={track.id}
                        disabled={track.locked}
                      >
                        {track.name} · {track.kind}
                      </option>
                    ))}
                </select>
              </Field>
              <Field
                label={t("New start for {name} (seconds)", { name: row.name })}
              >
                <input
                  inputMode="decimal"
                  value={row.start}
                  onChange={(e) =>
                    setRows((current) =>
                      current.map((item, i) =>
                        i === index
                          ? { ...item, start: e.target.value, edited: true }
                          : item,
                      ),
                    )
                  }
                />
              </Field>
              <small>
                {t("Current start")}: {formatTime(row.initial)}
              </small>
            </fieldset>
          ))}
        {action === "trim" && (
          <Field label={t("Trim edge")}>
            <select
              value={edge}
              onChange={(e) => setEdge(e.target.value as "left" | "right")}
            >
              <option value="left">{t("Left")}</option>
              <option value="right">{t("Right")}</option>
            </select>
          </Field>
        )}
        {(action === "trim" || action === "split") && (
          <Field label={t("Boundary (scene seconds)")}>
            <input
              inputMode="decimal"
              value={time}
              onChange={(e) => {
                setTime(e.target.value);
                setTimeEdited(true);
              }}
            />
          </Field>
        )}
        {action === "split" && (
          <Field label={t("Keep after split")}>
            <select
              value={retain}
              onChange={(e) => setRetain(e.target.value as typeof retain)}
            >
              <option value="both">{t("Both parts")}</option>
              <option value="left">{t("Left part")}</option>
              <option value="right">{t("Right part")}</option>
            </select>
          </Field>
        )}
        {action === "delete" && (
          <ul>
            {rows.map((row) => (
              <li key={row.target.element_id}>{row.name}</li>
            ))}
          </ul>
        )}
        {action === "add_track" ? (
          <>
            <Field label={t("Track name")}>
              <input
                value={trackName}
                onChange={(e) => setTrackName(e.target.value)}
              />
            </Field>
            <Field label={t("Track type")}>
              <select
                value={trackKind}
                onChange={(e) =>
                  setTrackKind(e.target.value as typeof trackKind)
                }
              >
                <option value="video">{t("Video")}</option>
                <option value="audio">{t("Audio")}</option>
                <option value="graphic">{t("Graphic")}</option>
                <option value="text">{t("Text")}</option>
                <option value="effect">{t("Effect")}</option>
                <option value="caption">{t("Caption")}</option>
              </select>
            </Field>
          </>
        ) : (
          <Field label={t("Ripple affected tracks")}>
            <input
              type="checkbox"
              checked={ripple}
              onChange={(e) => setRipple(e.target.checked)}
            />
          </Field>
        )}
        {(action === "trim" || action === "split") && (
          <p className="muted">
            {t(
              "Animated trim and split use the complete timeline planner. Legacy lookup curves require explicit conversion.",
            )}
          </p>
        )}
        <p className="muted">
          {t(
            "Review the planned changes before saving. Timing and ripple are validated by the editor service.",
          )}
        </p>
        {stale && (
          <p role="alert">
            {t(
              "The timeline target changed. Close this form and review a new operation.",
            )}
          </p>
        )}
        {error && <p role="alert">{t(error)}</p>}
        <div className="toolbar">
          <Button onClick={close}>{t("Cancel")}</Button>
          <Button
            disabled={
              pending ||
              props.disabled ||
              stale ||
              (action === "add_track" && !trackName.trim())
            }
            onClick={() => void submit()}
          >
            {t("Review timeline changes")}
          </Button>
        </div>
      </div>
    </Modal>
  );
}
