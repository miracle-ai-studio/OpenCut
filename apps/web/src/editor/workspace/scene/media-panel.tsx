import { useResourceDrag } from "./workbench/resource-drag";
import { useEffect, useRef, useState } from "react";
import { Button, Empty, Modal, Panel } from "../../shared/ui";
import { t, useLocale } from "../../shared/locale";
import { formatTime } from "../../model/time";
import { useEditingWorkspace as useWorkspace } from "../context";
import {
  applyMediaPlan,
  placeMediaDirect,
  MediaPlanDialog,
  PlaceMediaDialog,
} from "./media-dialog";
import { ResourcePreview } from "./resource-preview";
import type { Asset } from "./types";
import { SourceArtView } from "./source-art/view";
import { useShortcutActions } from "../../shortcuts/provider";
import type { Session } from "./session";
import { useResourceCopy } from "./workbench/resources-copy";
import { Plus, RefreshCw, Trash2 } from "lucide-react";
import {CardMenu} from "./workbench/card-menu";

type RemovalReview = Awaited<ReturnType<Session["planMediaRemoval"]>>;

export function MediaPanel({
  filter = "all",
}: { filter?: "all" | "video" | "audio" | "image" } = {}) {
  useLocale();
  const resourceDrag = useResourceDrag();
  const c = useResourceCopy();
  const { controller, scene, busy, api } = useWorkspace();
  const input = useRef<HTMLInputElement>(null),
    replacement = useRef<Asset | null>(null);
  const artworkActionPointer = useRef(false);
  const [placing, setPlacing] = useState<Asset | null>(null),
    [preview, setPreview] = useState<{
      asset: Asset;
      projectId: string;
      revision: string;
    } | null>(null);
  const [review, setReview] = useState(false),
    [search, setSearch] = useState("");
  const [selected, setSelected] = useState<string[]>([]),
    [removal, setRemoval] = useState<RemovalReview | null>(null),
    [removing, setRemoving] = useState(false),
    [removalError, setRemovalError] = useState<string | null>(null);
  const removalEpoch = useRef(0);
  const projectId = scene?.head.project.id;
  useEffect(() => {
    if (api && projectId) void controller.scene.media.refreshAvailable({ api, projectId });
  }, [controller, api, projectId]);
  useEffect(() => {
    setPlacing(null);
    setPreview(null);
    setReview(false);
    setSelected([]);
    setRemoval(null);
    setRemovalError(null);
    removalEpoch.current++;
  }, [projectId, scene?.sequenceId, scene?.historical?.revision]);
  useEffect(
    () => () => {
      removalEpoch.current++;
    },
    [],
  );
  const selectedAssets = selected.filter((id) =>
    scene?.head.project.assets.some((a) => a.id === id),
  );
  const removalDisabled =
    !scene ||
    !!scene.historical ||
    busy ||
    removing ||
    !!removal ||
    !!scene.media.attempt ||
    !!scene.recovery?.record ||
    !!scene.recovery?.error;
  const planRemoval = async (ids: string[]) => {
    if (removalDisabled || !ids.length) return;
    const epoch = removalEpoch.current;
    setRemoving(true);
    setRemovalError(null);
    try {
      const result = await controller.scene.planMediaRemoval(ids);
      if (epoch === removalEpoch.current) setRemoval(result);
    } catch (error) {
      if (epoch === removalEpoch.current)
        setRemovalError(error instanceof Error ? error.message : String(error));
    } finally {
      setRemoving(false);
    }
  };
  useShortcutActions({
    "remove-media-asset": {
      enabled: !removalDisabled && selectedAssets.length === 1,
      reason: t("Select one media asset first."),
      run: () => planRemoval(selectedAssets),
    },
    "remove-media-assets": {
      enabled: !removalDisabled && selectedAssets.length > 0,
      reason: t("Select media assets first."),
      run: () => planRemoval(selectedAssets),
    },
  });
  if (!scene) return null;
  const view = scene.historical ?? scene.head,
    media = scene.media,
    attempt = media.attempt;
  const disabled =
    !!scene.propertyDrafts?.length ||
    !!scene.historical ||
    busy ||
    !!attempt ||
    !!scene.recovery?.record ||
    !!scene.recovery?.error;
  const choose = (asset: Asset | null = null) => {
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
      const plan = await controller.scene.media.importFiles(
        files,
        replacing ?? undefined,
      );
      await applyMediaPlan(controller, api!, plan);
    } catch (error) {
      controller.report(error);
    } finally {
      replacement.current = null;
      if (input.current) {
        input.current.value = "";
        input.current.multiple = true;
      }
      // Only failures keep an actionable original request on screen.
      if (
        controller.getSnapshot().api === api &&
        controller.getSnapshot().scene?.head.project.id === projectId
      )
        setReview(!!controller.getSnapshot().scene?.media.attempt);
    }
  };
  const assets = view.project.assets.filter(
    (asset) =>
      asset.name.toLowerCase().includes(search.toLowerCase()) &&
      (filter === "all" || asset.kind === filter),
  );
  return (
    <Panel
      title={t("Media")}
      className="editor-media-panel"
      actions={
        <>
          <Button
            className="primary"
            icon={<Plus size={14} />}
            disabled={disabled}
            onClick={() => choose()}
          >
            {t("Import media")}
          </Button>
          <Button
            title={t("Remove selected media")}
            icon={<Trash2 size={14} />}
            className="workbench-media-remove"
            disabled={removalDisabled || !selectedAssets.length}
            onClick={() => void planRemoval(selectedAssets)}
          >
            {t("Remove selected media")}
          </Button>
          <Button
            aria-label={t("Refresh available media")}
            title={t("Refresh available media")}
            icon={<RefreshCw size={14} />}
            disabled={media.loading}
            onClick={() => { if (api && projectId) void controller.scene.media.refreshAvailable({ api, projectId }); }}
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
        className="editor-media-library scroll"
        data-shortcut-scope="assets"
        tabIndex={0}
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
        {removalError && <p role="alert">{t(removalError)}</p>}
        {attempt && (
          <div className="editor-media-status" role="status">
            {attempt.phase === "uploading" ? (
              <>
                <span>{attempt.progress ?? t("Preparing media…")}</span>
                <Button onClick={() => controller.scene.media.cancelUpload()}>
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
                        : attempt.phase === "failed" ? "Media changes failed." : c("saving"),
                  )}
                </span>
                <Button onClick={() => setReview(true)}>{c("recover")}</Button>
              </>
            )}
          </div>
        )}
        <div className="editor-media-grid">
          {!assets.length && (
            <Empty
              title={t(
                view.project.assets.length
                  ? "No matching media"
                  : "Bring your story in",
              )}
            >
              {t(
                view.project.assets.length
                  ? "Try another category or search term."
                  : "Drop video, images or audio here.",
              )}
            </Empty>
          )}
          {assets.map((asset) => (
            <article
              className="editor-media-card"
              draggable={!!resourceDrag && !resourceDrag.disabled}
              onPointerDownCapture={event => { artworkActionPointer.current = event.target instanceof Element && !!event.target.closest(".source-art-actions"); }}
              onDragStart={event => {
                if (artworkActionPointer.current) { event.preventDefault(); event.stopPropagation(); return; }
                resourceDrag?.start(event, {kind:"media", assetId:asset.id, name:asset.name});
              }}
              onDragEnd={() => resourceDrag?.cancel()}
              onDoubleClick={event=>{if(event.target instanceof Element&&event.target.closest("button,input,label"))return;setPreview({asset,projectId:view.project.id,revision:view.revision});}}
              key={asset.id}
              data-selected={selectedAssets.includes(asset.id) || undefined}
            >
              <label>
                <input
                  type="checkbox"
                  aria-label={t("Select media {name}", { name: asset.name })}
                  disabled={removalDisabled}
                  checked={selectedAssets.includes(asset.id)}
                  onChange={(event) =>
                    setSelected(
                      event.target.checked
                        ? [...selectedAssets, asset.id]
                        : selectedAssets.filter((id) => id !== asset.id),
                    )
                  }
                />
                {t("Select")}
              </label>
              {["video", "image", "audio"].includes(asset.kind) && (
                <SourceArtView
                  api={api}
                  onPreview={() => setPreview({ asset, projectId: view.project.id, revision: view.revision })}
                  selection={{
                    projectId: view.project.id,
                    revision: view.revision,
                    asset,
                    query:
                      asset.kind === "audio"
                        ? {
                            kind: "waveform",
                            sample_rate: 48000,
                            projection: {
                              kind: "source",
                              range: null,
                              bins: 128,
                            },
                          }
                        : { kind: "thumbnail" },
                  }}
                />
              )}
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
              <div className="toolbar resource-media-actions">
                <CardMenu name={asset.name} items={[
                  {label:t("Preview source"),action:()=>setPreview({asset,projectId:view.project.id,revision:view.revision})},
                  {label:t("Add to timeline"),disabled,action:async()=>{
                    try {await placeMediaDirect(controller,asset);}catch(error){controller.report(error);}
                    finally {if(controller.getSnapshot().api===api&&controller.getSnapshot().scene?.head.project.id===projectId)setReview(!!controller.getSnapshot().scene?.media.attempt);}
                  }},
                  {label:c("addSettings"),disabled,action:()=>setPlacing(asset)},
                  {label:t("Replace {name}",{name:asset.name}),disabled,action:()=>choose(asset)},
                  {label:t("Remove {name}",{name:asset.name}),disabled:removalDisabled,danger:true,action:()=>void planRemoval([asset.id])},
                ]}/>
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
            <div className="editor-available-item" key={item.media_id}>
              <span>{item.asset.name}</span>
              <Button
                aria-label={t("Recover {name}", { name: item.asset.name })}
                disabled={disabled || !item.asset.version}
                onClick={async () => {
                  try {
                    const plan = await controller.scene.media.recover(item);
                    await applyMediaPlan(controller, api!, plan);
                  } catch (error) {
                    controller.report(error);
                  } finally {
                    if (
                      controller.getSnapshot().api === api &&
                      controller.getSnapshot().scene?.head.project.id ===
                        projectId
                    )
                      setReview(
                        !!controller.getSnapshot().scene?.media.attempt,
                      );
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
            setReview(!!controller.getSnapshot().scene?.media.attempt);
          }}
        />
      )}
      {preview && (
        <ResourcePreview {...preview} onClose={() => setPreview(null)} />
      )}
      {review && attempt && (
        <MediaPlanDialog onClose={() => setReview(false)} />
      )}
      {removal && (
        <Modal
          title={t("Remove media from project")}
          onClose={() => {
            if (!removing) setRemoval(null);
          }}
        >
          <p>
            {t(
              "Remove these media assets and their clips from all scenes? Caption text is kept. Undo can restore the change.",
            )}
          </p>
          <ul>
            {removal.assetIds.map((id) => (
              <li key={id}>
                {view.project.assets.find((asset) => asset.id === id)?.name ??
                  id}
              </li>
            ))}
          </ul>
          {removal.transaction.expected_revision !== scene.head.revision && (
            <p role="alert">
              {t(
                "The project changed. Close this review and plan the removal again.",
              )}
            </p>
          )}
          {removalError && <p role="alert">{t(removalError)}</p>}
          <div className="toolbar">
            <Button disabled={removing} onClick={() => setRemoval(null)}>
              {t("Cancel")}
            </Button>
            <Button
              disabled={
                removing ||
                busy ||
                !!scene.historical ||
                !!scene.recovery?.record ||
                !!scene.recovery?.error ||
                removal.transaction.expected_revision !== scene.head.revision
              }
              onClick={async () => {
                setRemoving(true);
                setRemovalError(null);
                try {
                  await controller.scene.applyPlanned(
                    removal.transaction,
                    removal.sequenceId,
                  );
                  setSelected([]);
                  setRemoval(null);
                } catch (error) {
                  setRemovalError(
                    error instanceof Error ? error.message : String(error),
                  );
                  if (controller.getSnapshot().scene?.recovery?.record)
                    setRemoval(null);
                } finally {
                  setRemoving(false);
                }
              }}
            >
              {t("Confirm media removal")}
            </Button>
          </div>
        </Modal>
      )}
    </Panel>
  );
}
