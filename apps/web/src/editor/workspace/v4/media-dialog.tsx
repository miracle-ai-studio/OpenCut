import { useState } from "react";
import { Button, Field, Modal } from "../../shared/ui";
import { t, useLocale } from "../../shared/locale";
import { useWorkspace } from "../context";
import {
  formatTime,
  secondsForDisplay,
  timeFromDecimalSeconds,
} from "../../model/time";
import type { V4Asset, V4MediaRequest } from "./types";

export function PlaceMediaDialog({
  asset,
  onClose,
  onPlanned,
}: {
  asset: V4Asset;
  onClose(): void;
  onPlanned(): void;
}) {
  useLocale();
  const { controller, v4, busy } = useWorkspace();
  const [scope] = useState(() => ({
    projectId: v4!.head.project.id,
    revision: v4!.head.revision,
    sequenceId: v4!.sequenceId,
  }));
  const [initialTime] = useState(v4!.playhead);
  const kind =
    asset.kind === "audio"
      ? "audio"
      : asset.kind === "video"
        ? "video"
        : "image";
  const trackKind = kind === "audio" ? "audio" : "video";
  const sequence = v4?.head.project.sequences.find(
    (s) => s.id === scope.sequenceId,
  );
  const compatible =
    sequence?.tracks.filter(
      (track) =>
        !track.locked &&
        (kind === "audio"
          ? track.kind === "audio"
          : kind === "video"
            ? track.kind === "video"
            : ["video", "graphic"].includes(track.kind)),
    ) ?? [];
  const [target, setTarget] = useState(
    compatible.find((track) => track.id === v4?.selectedId)?.id ??
      compatible[0]?.id ??
      "new",
  );
  const [trackName, setTrackName] = useState(
    t(trackKind === "audio" ? "Audio track" : "Video track"),
  );
  const [start, setStart] = useState(secondsForDisplay(initialTime).toFixed(6)),
    [startEdited, setStartEdited] = useState(false);
  const [sourceStart, setSourceStart] = useState("0"),
    [duration, setDuration] = useState(
      asset.duration ? secondsForDisplay(asset.duration).toFixed(6) : "3",
    );
  const [durationEdited, setDurationEdited] = useState(false);
  const [numerator, setNumerator] = useState("1"),
    [denominator, setDenominator] = useState("1");
  const [keepPitch, setKeepPitch] = useState(true),
    [sourceAudio, setSourceAudio] = useState(
      asset.metadata.audio.status === "present",
    );
  const [pending, setPending] = useState(false),
    [error, setError] = useState<string | null>(null);
  const stale =
    !v4 ||
    !!v4.historical ||
    v4.head.project.id !== scope.projectId ||
    v4.head.revision !== scope.revision ||
    v4.sequenceId !== scope.sequenceId;
  const unsupported =
    !["video", "audio", "image", "graphic"].includes(asset.kind) ||
    (kind !== "image" && !asset.duration);
  const submit = async () => {
    setPending(true);
    setError(null);
    try {
      const n = Number(numerator),
        d = Number(denominator);
      if (
        ![n, d].every(
          (value) =>
            Number.isInteger(value) && value > 0 && value <= 4294967295,
        )
      )
        throw new Error(
          "Speed numerator and denominator must be positive integers.",
        );
      const timing = {
        kind: "source_range" as const,
        range: {
          start: timeFromDecimalSeconds(sourceStart),
          duration:
            !durationEdited && asset.duration
              ? asset.duration
              : timeFromDecimalSeconds(duration),
        },
        rate: { numerator: n, denominator: d },
        maintain_pitch: keepPitch,
      };
      const content: V4MediaRequest["placements"][number]["content"] =
        kind === "image"
          ? {
              kind: "image",
              duration:
                !durationEdited && asset.duration
                  ? asset.duration
                  : timeFromDecimalSeconds(duration),
            }
          : kind === "audio"
            ? { kind: "audio", timing }
            : { kind: "video", timing, source_audio_enabled: sourceAudio };
      await controller.v4.media.place(
        {
          name: asset.name,
          asset_id: asset.id,
          track_id: target,
          start: startEdited ? timeFromDecimalSeconds(start) : initialTime,
          content,
        },
        target === "new" || target === "new_graphic"
          ? {
              kind: target === "new_graphic" ? "graphic" : trackKind,
              name: trackName,
            }
          : undefined,
        scope,
      );
      onPlanned();
    } catch (error) {
      setError(error instanceof Error ? error.message : String(error));
    } finally {
      setPending(false);
    }
  };
  return (
    <Modal title={t("Place media")} onClose={onClose}>
      <p>{asset.name}</p>
      <p className="muted">
        {t(
          "Review the service's resolved placement before saving. Still scene frames depend on this service’s rendering capability. Playback remains unavailable.",
        )}
      </p>
      {stale && (
        <p role="alert">
          {t(
            "The scene or revision changed. Close this form and choose media again.",
          )}
        </p>
      )}
      {unsupported && (
        <p role="alert">
          {t(
            "This media needs supported type and observed duration before placement.",
          )}
        </p>
      )}
      <div className="v4-media-fields">
        <Field label={t("Target track")}>
          <select value={target} onChange={(e) => setTarget(e.target.value)}>
            {compatible.map((track) => (
              <option key={track.id} value={track.id}>
                {track.name}
              </option>
            ))}
            <option value="new">
              {t(trackKind === "audio" ? "New audio track" : "New video track")}
            </option>
            {kind === "image" && (
              <option value="new_graphic">{t("New graphic track")}</option>
            )}
          </select>
        </Field>
        {(target === "new" || target === "new_graphic") && (
          <Field label={t("New track name")}>
            <input
              value={trackName}
              onChange={(e) => setTrackName(e.target.value)}
            />
          </Field>
        )}
        <Field label={t("Timeline start (seconds)")}>
          <input
            inputMode="decimal"
            value={start}
            onChange={(e) => {
              setStart(e.target.value);
              setStartEdited(true);
            }}
          />
        </Field>
        {kind !== "image" && (
          <Field label={t("Source start (seconds)")}>
            <input
              inputMode="decimal"
              value={sourceStart}
              onChange={(e) => setSourceStart(e.target.value)}
            />
          </Field>
        )}
        <Field
          label={t(
            kind === "image"
              ? "Image duration (seconds)"
              : "Source duration (seconds)",
          )}
        >
          <input
            inputMode="decimal"
            value={duration}
            onChange={(e) => {
              setDuration(e.target.value);
              setDurationEdited(true);
            }}
          />
        </Field>
        {kind !== "image" && (
          <>
            <Field label={t("Speed numerator")}>
              <input
                inputMode="numeric"
                value={numerator}
                onChange={(e) => setNumerator(e.target.value)}
              />
            </Field>
            <Field label={t("Speed denominator")}>
              <input
                inputMode="numeric"
                value={denominator}
                onChange={(e) => setDenominator(e.target.value)}
              />
            </Field>
            <label>
              <input
                type="checkbox"
                checked={keepPitch}
                onChange={(e) => setKeepPitch(e.target.checked)}
              />{" "}
              {t("Maintain pitch")}
            </label>
          </>
        )}
        {kind === "video" && (
          <label>
            <input
              type="checkbox"
              checked={sourceAudio}
              disabled={asset.metadata.audio.status !== "present"}
              onChange={(e) => setSourceAudio(e.target.checked)}
            />{" "}
            {t("Include source audio")}
          </label>
        )}
        {kind === "video" && asset.metadata.audio.status !== "present" && (
          <p className="muted">
            {t(
              asset.metadata.audio.status === "absent"
                ? "This video has no source audio."
                : "Audio presence has not been verified.",
            )}
          </p>
        )}
      </div>
      {error && <p role="alert">{t(error)}</p>}
      <div className="modal-footer">
        <Button onClick={onClose}>{t("Close")}</Button>
        <Button
          disabled={stale || unsupported || busy || pending}
          onClick={() => void submit()}
        >
          {t("Review placement")}
        </Button>
      </div>
    </Modal>
  );
}

export function MediaPlanDialog({ onClose }: { onClose(): void }) {
  useLocale();
  const { controller, v4, busy } = useWorkspace(),
    attempt = v4?.media.attempt;
  const [pending, setPending] = useState(false),
    [error, setError] = useState<string | null>(null);
  if (!v4 || !attempt) return null;
  const plan = attempt.plan,
    decisions = plan?.decisions;
  const changed =
    !!v4.historical ||
    v4.sequenceId !== attempt.request.sequence_id ||
    v4.head.revision !== attempt.request.expected_revision;
  const run = async (action: () => Promise<unknown>) => {
    setPending(true);
    setError(null);
    try {
      await action();
    } catch (error) {
      setError(error instanceof Error ? error.message : String(error));
    } finally {
      setPending(false);
    }
  };
  const fps = (value: { numerator: number; denominator: number }) =>
    `${value.numerator}/${value.denominator}`;
  return (
    <Modal title={t("Review media changes")} onClose={onClose}>
      <p>
        {t("Scene")}:{" "}
        {v4.head.project.sequences.find(
          (s) => s.id === attempt.request.sequence_id,
        )?.name ?? t("Unavailable scene")}
      </p>
      <p className="muted">
        {t("Saved revision")}: {attempt.request.expected_revision}
      </p>
      {attempt.request.bindings.map((binding) => (
        <p key={binding.asset_id}>
          {t(binding.mode === "replace" ? "Replace media" : "Bind media")}:{" "}
          {binding.name}
        </p>
      ))}
      {decisions && (
        <>
          <p>
            {t("Frame rate")}: {fps(decisions.previous_fps)} →{" "}
            {fps(
              decisions.first_visual_fps ??
                decisions.raised_fps ??
                decisions.previous_fps,
            )}
          </p>
          {decisions.canvas_change && (
            <p>
              {t("Canvas")}: {decisions.canvas_change.previous.width} ×{" "}
              {decisions.canvas_change.previous.height} →{" "}
              {decisions.canvas_change.next.width} ×{" "}
              {decisions.canvas_change.next.height}
            </p>
          )}
          {decisions.placements.map((placement) => (
            <p key={placement.element_id}>
              {t("Placement")}: {formatTime(placement.resolved_start)} ·{" "}
              {t("Duration")}: {formatTime(placement.timeline_duration)}
            </p>
          ))}
          {decisions.video_asset_ids_without_fps.length > 0 && (
            <p>
              {t(
                "Some video frame rates are unknown; the service preserved its declared settings.",
              )}
            </p>
          )}
        </>
      )}
      {attempt.request.new_tracks.map((track) => (
        <p key={track.id}>
          {t("New track")}: {track.name}
        </p>
      ))}
      {changed && attempt.phase !== "saved" && (
        <p role="status">
          {t(
            "The current view differs from this plan. Return to its scene; a changed revision requires a new review.",
          )}
        </p>
      )}
      {attempt.phase === "unconfirmed" && (
        <p role="alert">
          {t(
            "Media changes are not confirmed. Retry the original changes to recover their receipt; do not import again.",
          )}
        </p>
      )}
      {attempt.phase === "saved" && (
        <p role="status">{t("Media changes saved.")}</p>
      )}
      {(error || attempt.error) && (
        <p role="alert">{t(error ?? attempt.error!)}</p>
      )}
      {attempt.phase === "saved" ? (
        <div className="modal-footer">
          {attempt.error && (
            <Button
              onClick={() => void run(() => controller.v4.media.refreshSaved())}
            >
              {t("Refresh saved changes")}
            </Button>
          )}
          <Button
            onClick={() => {
              controller.v4.media.dismiss();
              onClose();
            }}
          >
            {t("Done")}
          </Button>
        </div>
      ) : (
        <div className="modal-footer">
          <Button onClick={onClose}>{t("Close")}</Button>
          {!["unconfirmed", "uploading", "planning", "applying"].includes(
            attempt.phase,
          ) && (
            <Button
              disabled={busy || pending}
              onClick={() => {
                controller.v4.media.dismiss();
                onClose();
              }}
            >
              {t("Discard media plan")}
            </Button>
          )}
          {plan && (
            <Button
              disabled={
                busy ||
                pending ||
                !!v4.historical ||
                v4.sequenceId !== attempt.request.sequence_id ||
                (changed && attempt.phase !== "unconfirmed")
              }
              onClick={() => void run(() => controller.v4.media.apply())}
            >
              {t(
                attempt.phase === "unconfirmed" || attempt.phase === "failed"
                  ? "Retry original media changes"
                  : "Apply media changes",
              )}
            </Button>
          )}
          {!plan &&
            attempt.phase === "failed" &&
            (attempt.request.bindings.length > 0 ||
              attempt.request.placements.length > 0) && (
              <Button
                disabled={busy || pending || changed}
                onClick={() => void run(() => controller.v4.media.retryPlan())}
              >
                {t("Retry media planning")}
              </Button>
            )}
        </div>
      )}
    </Modal>
  );
}
