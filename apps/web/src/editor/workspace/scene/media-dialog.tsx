import { Select } from "../../shared/controls";
import { useState } from "react";
import { Button, Field, Modal } from "../../shared/ui";
import { t, useLocale } from "../../shared/locale";
import { useWorkspace } from "../context";
import {
  formatTime,
  addTime, compareTime,
  secondsForDisplay,
  timeFromDecimalSeconds,
} from "../../model/time";
import type { WorkspaceController } from "../controller";
import type { EditorApi } from "../api";
import { useResourceCopy } from "./workbench/resources-copy";
import type { Asset, MediaPlan, MediaRequest, Sequence } from "./types";
import { zero } from "../../model/time";

export type MediaSourceRange = Sequence["tracks"][number]["elements"][number]["timeline_range"];
export function mediaPlacementTracks(sequence: Sequence | undefined, kind: Asset["kind"]) {
  return sequence?.tracks.filter(track => !track.locked && track.enabled &&
    (kind === "audio" ? track.kind === "audio" : kind === "video" ? track.kind === "video" : ["video", "graphic"].includes(track.kind))) ?? [];
}

export function PlaceMediaDialog({
  asset,
  onClose,
  onPlanned,
}: {
  asset: Asset;
  onClose(): void;
  onPlanned(): void;
}) {
  useLocale();
  const c = useResourceCopy();
  const { controller, scene, busy, api } = useWorkspace();
  const [scope] = useState(() => ({
    api,
    connectionIdentity: api?.connectionIdentity,
    projectId: scene!.head.project.id,
    revision: scene!.head.revision,
    sequenceId: scene!.sequenceId,
  }));
  const [initialTime] = useState(()=>controller.scene.currentPlayhead());
  const kind =
    asset.kind === "audio"
      ? "audio"
      : asset.kind === "video"
        ? "video"
        : "image";
  const trackKind = kind === "audio" ? "audio" : "video";
  const sequence = scene?.head.project.sequences.find(
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
    compatible.find((track) => track.id === scene?.selectedId)?.id ??
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
    !scene ||
    api !== scope.api ||
    api?.connectionIdentity !== scope.connectionIdentity ||
    !!scene.historical ||
    scene.head.project.id !== scope.projectId ||
    scene.head.revision !== scope.revision ||
    scene.sequenceId !== scope.sequenceId;
  const unsupported =
    !["video", "audio", "image", "graphic"].includes(asset.kind) ||
    (kind !== "image" && !asset.duration);
  const submit = async () => {
    setPending(true);
    setError(null);
    try {
      if (stale || controller.getSnapshot().api !== scope.api)
        throw Error(
          "The media target changed. Return to the original scene to continue.",
        );
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
      const content: MediaRequest["placements"][number]["content"] =
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
      const plan = await controller.scene.media.place(
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
      await applyMediaPlan(controller, api!, plan);
      onPlanned();
    } catch (error) {
      if (
        controller.getSnapshot().api === api &&
        controller.getSnapshot().scene?.head.project.id === scope.projectId &&
        controller.getSnapshot().scene?.media.attempt
      )
        onPlanned();
      setError(error instanceof Error ? error.message : String(error));
    } finally {
      setPending(false);
    }
  };
  return (
    <Modal title={t("Place media")} onClose={onClose}>
      <p>{asset.name}</p>
      <p className="muted">{c("placementHint")}</p>
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
      <div className="editor-media-fields">
        <Field label={t("Target track")}>
          <Select value={target} onChange={(e) => setTarget(e.target.value)}>
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
          </Select>
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
          {c("add")}
        </Button>
      </div>
    </Modal>
  );
}

/** Apply only the exact server plan returned to this user action. A changed
 * connection, scene or request must not silently apply a newer intent. */
export async function applyMediaPlan(
  controller: WorkspaceController,
  api: EditorApi,
  plan: MediaPlan,
) {
  const state = controller.getSnapshot(),
    scene = state.scene,
    attempt = scene?.media.attempt;
  if (
    state.api !== api ||
    !scene ||
    scene.historical ||
    !attempt ||
    attempt.plan !== plan ||
    attempt.request.idempotency_key !== plan.transaction.idempotency_key ||
    scene.head.project.id !== plan.transaction.project_id ||
    scene.sequenceId !== attempt.request.sequence_id ||
    scene.head.revision !== plan.transaction.expected_revision
  )
    throw Error(
      "The media target changed. Return to the original scene to continue.",
    );
  await controller.scene.media.apply();
  const after = controller.getSnapshot(),
    saved = after.scene?.media.attempt;
  if (
    after.api === api &&
    after.scene?.head.project.id === plan.transaction.project_id &&
    saved?.request.idempotency_key === plan.transaction.idempotency_key &&
    saved.phase === "saved" &&
    !saved.error
  )
    controller.scene.media.dismiss();
}

export async function placeMediaDirect(
  controller: WorkspaceController,
  asset: Asset,
  position?: {
    projectId: string;
    revision: string;
    sequenceId: string;
    at: MediaRequest["placements"][number]["start"];
    trackId?: string | null;
  },
  sourceRange?: MediaSourceRange,
) {
  const { api, scene, busy } = controller.getSnapshot();
  if (
    !api ||
    !scene ||
    busy ||
    scene.historical ||
    scene.media.attempt ||
    scene.recovery?.record ||
    scene.recovery?.error
  )
    throw Error(
      "Resolve the current media changes before starting another operation.",
    );
  if (
    position &&
    (position.projectId !== scene.head.project.id ||
      position.revision !== scene.head.revision ||
      position.sequenceId !== scene.sequenceId)
  )
    throw Error(
      "The media target changed. Return to the original scene to continue.",
    );
  if (
    !["image", "graphic", "audio", "video"].includes(asset.kind) ||
    (["audio", "video"].includes(asset.kind) && !asset.duration)
  )
    throw Error(
      "This media needs supported type and observed duration before placement.",
    );
  const kind =
    asset.kind === "audio"
      ? "audio"
      : asset.kind === "video"
        ? "video"
        : "image";
  const sequence = scene.head.project.sequences.find(
    (sequence) => sequence.id === scene.sequenceId,
  );
  const compatible = mediaPlacementTracks(sequence, asset.kind);
  const track = position?.trackId !== undefined
    ? compatible.find((track) => track.id === position.trackId)
    : compatible.find(
      (track) =>
        track.id === scene.selectedId ||
        track.elements.some((element) => element.id === scene.selectedId),
    ) ?? compatible[0];
  if (position?.trackId && !track) throw Error("Choose a compatible unlocked track.");
  if (sourceRange && (kind === "image" || !asset.duration ||
    compareTime(sourceRange.start, zero) < 0 || compareTime(sourceRange.duration, zero) <= 0 ||
    compareTime(addTime(sourceRange.start, sourceRange.duration), asset.duration) > 0))
    throw Error("Choose a source range within the observed media duration.");
  const duration = sourceRange?.duration ?? asset.duration ?? { ticks: "3", timebase: 1 };
  if (position?.trackId !== undefined && track && track.elements.some(element =>
    compareTime(position.at, addTime(element.timeline_range.start, element.timeline_range.duration)) < 0 &&
    compareTime(addTime(position.at, duration), element.timeline_range.start) > 0))
    throw Error("This range overlaps existing clips. Choose an empty range.");
  const timing = {
    kind: "source_range" as const,
    range: sourceRange ?? { start: { ticks: "0", timebase: 1 }, duration },
    rate: { numerator: 1, denominator: 1 },
    maintain_pitch: true,
  };
  const content: MediaRequest["placements"][number]["content"] =
    kind === "image"
      ? { kind, duration }
      : kind === "audio"
        ? { kind, timing }
        : {
            kind,
            timing,
            source_audio_enabled: asset.metadata.audio.status === "present",
          };
  const plan = await controller.scene.media.place(
    {
      name: asset.name,
      asset_id: asset.id,
      track_id: track?.id ?? "new",
      start: position?.at ?? controller.scene.currentPlayhead(),
      content,
    },
    track
      ? undefined
      : {
          kind: kind === "audio" ? "audio" : "video",
          name: t(kind === "audio" ? "Audio track" : "Video track"),
        },
    {
      projectId: scene.head.project.id,
      revision: scene.head.revision,
      sequenceId: scene.sequenceId,
    },
  );
  const elementId = controller.getSnapshot().scene?.media.attempt?.request.placements[0]?.element_id;
  await applyMediaPlan(controller, api, plan);
  const saved = controller.getSnapshot().scene;
  if (!elementId || !saved?.head.project.sequences.some(sequence => sequence.id === scene.sequenceId &&
    sequence.tracks.some(track => track.elements.some(element => element.id === elementId))))
    throw Error("The media placement is not confirmed. Resolve the original edit before retrying.");
  return { elementId };
}

/** Shown only for an interrupted/failed save or an explicitly reopened request. */
export function MediaPlanDialog({ onClose }: { onClose(): void }) {
  const c = useResourceCopy();
  const { controller, scene, busy, api } = useWorkspace(),
    attempt = scene?.media.attempt;
  const [pending, setPending] = useState(false),
    [error, setError] = useState<string | null>(null);
  if (!scene || !attempt || !api) return null;
  const changed =
    !!scene.historical ||
    scene.sequenceId !== attempt.request.sequence_id ||
    scene.head.revision !== attempt.request.expected_revision;
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
  const sequence = scene.head.project.sequences.find(
    (scene) => scene.id === attempt.request.sequence_id,
  );
  return (
    <Modal title={c("recoveryTitle")} onClose={onClose}>
      <p>{sequence?.name ?? t("Unavailable scene")}</p>
      {attempt.request.bindings.map((binding) => (
        <p key={binding.asset_id}>{binding.name}</p>
      ))}
      {attempt.request.placements.map((requested) => (
        <section
          key={requested.element_id}
          aria-label={t("Placement of {name}", { name: requested.name })}
        >
          <strong>{requested.name}</strong>
          <p>
            {t("Requested position")}: {formatTime(requested.start)}
          </p>
          <p>
            {t("Target track")}:{" "}
            {sequence?.tracks.find((track) => track.id === requested.track_id)
              ?.name ??
              attempt.request.new_tracks?.find(
                (track) => track.id === requested.track_id,
              )?.name ??
              t("Unavailable track")}
          </p>
        </section>
      ))}
      {changed && attempt.phase !== "saved" && (
        <p role="status">{c("changed")}</p>
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
      <div className="modal-footer">
        <Button disabled={pending} onClick={onClose}>
          {t("Close")}
        </Button>
        {attempt.phase === "saved" ? (
          <>
            {attempt.error && (
              <Button
                disabled={pending || busy}
                onClick={() =>
                  void run(() => controller.scene.media.refreshSaved())
                }
              >
                {t("Refresh saved changes")}
              </Button>
            )}
            <Button
              disabled={pending}
              onClick={() => {
                controller.scene.media.dismiss();
                onClose();
              }}
            >
              {t("Done")}
            </Button>
          </>
        ) : (
          <>
            {!["unconfirmed", "uploading", "planning", "applying"].includes(
              attempt.phase,
            ) && (
              <Button
                disabled={pending || busy}
                onClick={() => {
                  controller.scene.media.dismiss();
                  onClose();
                }}
              >
                {c("discard")}
              </Button>
            )}
            {attempt.plan && (
              <Button
                disabled={
                  pending ||
                  busy ||
                  !!scene.historical ||
                  scene.sequenceId !== attempt.request.sequence_id ||
                  (changed && attempt.phase !== "unconfirmed")
                }
                onClick={() =>
                  void run(async () => {
                    // Recovery deliberately uses the retained original request; an unknown
                    // outcome may be replayed after head movement to recover its receipt.
                    await controller.scene.media.apply();
                    const saved = controller.getSnapshot().scene?.media.attempt;
                    if (
                      controller.getSnapshot().api === api &&
                      saved?.request.idempotency_key ===
                        attempt.request.idempotency_key &&
                      saved.phase === "saved" &&
                      !saved.error
                    ) {
                      controller.scene.media.dismiss();
                      onClose();
                    }
                  })
                }
              >
                {c("retry")}
              </Button>
            )}
            {!attempt.plan &&
              attempt.phase === "failed" &&
              !!(
                attempt.request.bindings.length ||
                attempt.request.placements.length
              ) && (
                <Button
                  disabled={pending || busy || changed}
                  onClick={() =>
                    void run(async () => {
                      const plan = await controller.scene.media.retryPlan();
                      await applyMediaPlan(controller, api, plan);
                      onClose();
                    })
                  }
                >
                  {c("retry")}
                </Button>
              )}
          </>
        )}
      </div>
    </Modal>
  );
}
