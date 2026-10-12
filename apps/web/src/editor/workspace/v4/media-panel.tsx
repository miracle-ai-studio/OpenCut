import { useEffect, useRef, useState } from "react";
import { Button, Empty, Panel } from "../../shared/ui";
import { t, useLocale } from "../../shared/locale";
import { formatTime } from "../../model/time";
import { useWorkspace } from "../context";
import { MediaPlanDialog, PlaceMediaDialog } from "./media-dialog";
import { V4ResourcePreview } from "./resource-preview";
import type { V4Asset } from "./types";
import { SourceArtView } from "./source-art/view";

export function V4MediaPanel() {
  useLocale();
  const { controller, v4, busy, api } = useWorkspace();
  const input = useRef<HTMLInputElement>(null),
    replacement = useRef<V4Asset | null>(null);
  const [placing, setPlacing] = useState<V4Asset | null>(null),
    [preview, setPreview] = useState<{
      asset: V4Asset;
      projectId: string;
      revision: string;
    } | null>(null);
  const [review, setReview] = useState(false),
    [search, setSearch] = useState("");
  const projectId = v4?.head.project.id;
  useEffect(() => {
    if (projectId) void controller.v4.media.refreshAvailable();
  }, [controller, projectId]);
  useEffect(() => {
    setPlacing(null);
    setPreview(null);
    setReview(false);
  }, [projectId, v4?.sequenceId, v4?.historical?.revision]);
  if (!v4) return null;
  const view = v4.historical ?? v4.head,
    media = v4.media,
    attempt = media.attempt;
  const disabled = !!v4.historical || busy || !!attempt;
  const choose = (asset: V4Asset | null = null) => {
    replacement.current = asset;
    if (input.current) {
      input.current.multiple = !asset;
      input.current.click();
    }
  };
  const upload = async (files: File[]) => {
    if (disabled) return;
    const replacing = replacement.current;
    try {
      await controller.v4.media.importFiles(files, replacing ?? undefined);
    } catch (error) {
      controller.report(error);
    } finally {
      replacement.current = null;
      if (input.current) {
        input.current.value = "";
        input.current.multiple = true;
      }
      // Keep the captured command reviewable even if an upload or plan failed.
      if (controller.getSnapshot().v4?.head.project.id === projectId)
        setReview(true);
    }
  };
  const assets = view.project.assets.filter((asset) =>
    asset.name.toLowerCase().includes(search.toLowerCase()),
  );
  return (
    <Panel
      title={t("Media")}
      className="v4-media-panel"
      actions={
        <>
          <Button disabled={disabled} onClick={() => choose()}>
            {t("Import media")}
          </Button>
          <Button
            aria-label={t("Refresh available media")}
            disabled={media.loading}
            onClick={() => void controller.v4.media.refreshAvailable()}
          >
            {t("Refresh")}
          </Button>
        </>
      }
    >
      <input
        ref={input}
        hidden
        type="file"
        multiple
        aria-label={t("Import media files")}
        accept="video/*,audio/*,image/*"
        onChange={(event) => {
          if (event.target.files?.length)
            void upload(Array.from(event.target.files));
        }}
      />
      <div
        className="v4-media-library scroll"
        onDragOver={(event) => {
          if (!disabled && event.dataTransfer.types.includes("Files"))
            event.preventDefault();
        }}
        onDrop={(event) => {
          if (event.dataTransfer.files.length && !disabled) {
            event.preventDefault();
            replacement.current = null;
            void upload(Array.from(event.dataTransfer.files));
          }
        }}
      >
        <input
          aria-label={t("Search media")}
          placeholder={t("Search media")}
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
        {attempt && (
          <div className="v4-media-status" role="status">
            {attempt.phase === "uploading" ? (
              <>
                <span>{attempt.progress ?? t("Preparing media…")}</span>
                <Button onClick={() => controller.v4.media.cancelUpload()}>
                  {t("Cancel import")}
                </Button>
              </>
            ) : (
              <>
                <span>
                  {t(
                    attempt.phase === "saved"
                      ? "Media changes saved."
                      : attempt.phase === "unconfirmed"
                        ? "Media changes are not confirmed."
                        : "Media changes await review.",
                  )}
                </span>
                <Button onClick={() => setReview(true)}>
                  {t("Review media changes")}
                </Button>
              </>
            )}
          </div>
        )}
        <div className="v4-media-grid">
          {!assets.length && (
            <Empty title={t("Bring your story in")}>
              {t("Drop video, images or audio here.")}
            </Empty>
          )}
          {assets.map((asset) => (
            <article className="v4-media-card" key={asset.id}>
              {["video", "image", "audio"].includes(asset.kind) && <SourceArtView api={api} selection={{
                projectId: view.project.id, revision: view.revision, asset,
                query: asset.kind === "audio" ? { kind: "waveform", sample_rate: 48000,
                  projection: { kind: "source", range: null, bins: 128 } } : { kind: "thumbnail" },
              }} />}
              <strong>{asset.name}</strong>
              <small>
                {t(
                  asset.kind === "video"
                    ? "Video"
                    : asset.kind === "audio"
                      ? "Audio"
                      : asset.kind === "image"
                        ? "Image"
                        : asset.kind === "graphic"
                          ? "Graphic"
                          : "Unavailable media",
                )}{" "}
                {asset.duration ? `· ${formatTime(asset.duration)}` : ""}
              </small>
              <div className="toolbar">
                <Button
                  aria-label={t("Preview source {name}", { name: asset.name })}
                  disabled={
                    !["video", "audio", "image", "graphic"].includes(asset.kind)
                  }
                  onClick={() =>
                    setPreview({
                      asset,
                      projectId: view.project.id,
                      revision: view.revision,
                    })
                  }
                >
                  {t("Preview source")}
                </Button>
                <Button
                  aria-label={t("Place {name}", { name: asset.name })}
                  disabled={
                    disabled ||
                    !["video", "audio", "image", "graphic"].includes(asset.kind)
                  }
                  onClick={() => setPlacing(asset)}
                >
                  {t("Place media")}
                </Button>
                <Button
                  aria-label={t("Replace {name}", { name: asset.name })}
                  disabled={disabled}
                  onClick={() => choose(asset)}
                >
                  {t("Replace")}
                </Button>
              </div>
            </article>
          ))}
        </div>
        <details className="v4-available-media">
          <summary>
            {t("Available imported media")} · {media.available.length}
          </summary>
          <p className="muted">
            {t(
              "Completed uploads remain available if binding was cancelled or failed. Recover them without uploading again.",
            )}
          </p>
          {media.loading && <p role="status">{t("Loading media…")}</p>}
          {media.error && <p role="alert">{t(media.error)}</p>}
          {media.available.map((item) => (
            <div className="v4-available-item" key={item.media_id}>
              <span>{item.asset.name}</span>
              <Button
                aria-label={t("Recover {name}", { name: item.asset.name })}
                disabled={disabled || !item.asset.version}
                onClick={async () => {
                  try {
                    await controller.v4.media.recover(item);
                  } catch (error) {
                    controller.report(error);
                  } finally {
                    if (
                      controller.getSnapshot().v4?.head.project.id === projectId
                    )
                      setReview(true);
                  }
                }}
              >
                {t("Recover media")}
              </Button>
            </div>
          ))}
        </details>
      </div>
      {placing && (
        <PlaceMediaDialog
          key={placing.id}
          asset={placing}
          onClose={() => setPlacing(null)}
          onPlanned={() => {
            setPlacing(null);
            setReview(true);
          }}
        />
      )}
      {preview && (
        <V4ResourcePreview {...preview} onClose={() => setPreview(null)} />
      )}
      {review && attempt && (
        <MediaPlanDialog onClose={() => setReview(false)} />
      )}
    </Panel>
  );
}
