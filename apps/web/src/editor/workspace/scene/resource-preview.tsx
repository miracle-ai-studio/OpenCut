import { MediaPlayer } from "../../shared/controls";
import { useEffect, useState } from "react";
import { Modal } from "../../shared/ui";
import { t, useLocale } from "../../shared/locale";
import { useWorkspace } from "../context";
import type { Asset } from "./types";
import type { ToolOutput } from "../../../lib/editor-sdk";
import { useResourceCopy } from "./workbench/resources-copy";
import { usePlayerContext } from "./workbench/player-context";
import { SourcePlayer } from "./source-player";

/** Source media only. This never presents a project frame or mix as a preview. */
type ResourcePreviewProps = {
  asset: Asset; projectId: string; revision: string; onClose(): void; embedded?: boolean;
};
export function ResourcePreview(props: ResourcePreviewProps) {
  const player = usePlayerContext();
  useEffect(() => {
    if (player && !props.embedded) {
      void player.open({ kind: "source", asset: props.asset, projectId: props.projectId, revision: props.revision });
      props.onClose();
    }
  }, [player, props.asset, props.projectId, props.revision, props.embedded]);
  if (player && !props.embedded) return null;
  return <SourcePreviewContent {...props} />;
}
function SourcePreviewContent({
  asset,
  projectId,
  revision,
  onClose,
  embedded,
}: ResourcePreviewProps) {
  useLocale();
  const c = useResourceCopy();
  const { api } = useWorkspace();
  const [url, setUrl] = useState<string | null>(null),
    [error, setError] = useState<string | null>(null);
  const [recording, setRecording] =
    useState<ToolOutput<"opencut_catalog_resources_recipe"> | null>(null);
  useEffect(() => {
    const abort = new AbortController();
    setUrl(null);
    setError(null);
    setRecording(null);
    const load = async () => {
      if (!api) throw new Error("OpenCut is not ready.");
      const resource = await api.scene.resource(projectId, revision, asset.id);
      if (
        resource.project_id !== projectId ||
        resource.revision !== revision ||
        resource.resource_id !== asset.id ||
        resource.version.content_hash !== asset.version.content_hash ||
        resource.version.size !== asset.version.size ||
        resource.version.id !== asset.version.id
      )
        throw new Error("The selected media has changed. Refresh the project.");
      const source = api.sdk.projectResourceUrl(resource);
      if (new URL(source).origin !== location.origin)
        throw new Error("Source preview requires the local workspace.");
      let recipe: ToolOutput<"opencut_catalog_resources_recipe"> | null = null;
      if (
        asset.origin.kind === "generated" &&
        asset.origin.provider === "opencut-local-sfx-v1"
      ) {
        recipe = await api.sdk.catalogSoundRecipe(
          { project_id: projectId, revision, asset_id: asset.id },
          { signal: abort.signal },
        );
        if (
          recipe.project_id !== projectId ||
          recipe.revision !== revision ||
          recipe.asset_id !== asset.id ||
          recipe.version.id !== asset.version.id ||
          recipe.version.content_hash !== asset.version.content_hash ||
          recipe.version.size !== asset.version.size
        )
          throw new Error(
            "The recorded sound recipe has changed. Refresh the project.",
          );
      }
      abort.signal.throwIfAborted();
      setRecording(recipe);
      setUrl(source);
    };
    void load().catch((error) => {
      if (!abort.signal.aborted)
        setError(error instanceof Error ? error.message : String(error));
    });
    return () => abort.abort();
  }, [api, projectId, revision, asset]);
  const contents = <>
      {!embedded && <p>{asset.name}</p>}
      <p className="muted">
        {t(
          "Shows the original source media. Scene edits, notes and audio mixing are not included.",
        )}
      </p>
      {recording && (
        <details className="resource-details">
          <summary>{c("soundOptions")}</summary>
          <dl className="padded" aria-label={t("Recorded sound recipe")}>
            <dt>{t("Sound recipe")}</dt>
            <dd>
              {t(
                recording.recording.recipe.kind === "whoosh"
                  ? "Whoosh"
                  : recording.recording.recipe.kind === "click"
                    ? "Click"
                    : "Impact",
              )}
            </dd>
            <dt>{t("Duration (ms)")}</dt>
            <dd>{recording.recording.recipe.duration_ms}</dd>
            <dt>{c("variation")}</dt>
            <dd>{recording.recording.recipe.seed}</dd>
            <dt>{t("Level (dB)")}</dt>
            <dd>{recording.recording.recipe.level_db}</dd>
          </dl>
        </details>
      )}
      {error ? (
        <p role="alert">{t(error)}</p>
      ) : !url ? (
        <p role="status">{t("Loading source…")}</p>
      ) : embedded ? <SourcePlayer asset={asset} projectId={projectId} revision={revision} url={url} onPlaced={onClose} /> :
        asset.kind === "image" || asset.kind === "graphic" ? <img className="source-preview" src={url} alt={asset.name} /> : <MediaPlayer src={url} audioOnly={asset.kind === "audio"} />}
    </>;
  return embedded ? <div className="source-preview-content">{contents}</div> : <Modal title={t("Source media preview")} onClose={onClose}>{contents}</Modal>;
}

