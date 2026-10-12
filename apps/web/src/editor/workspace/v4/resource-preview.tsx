import { useEffect, useState } from "react";
import { Modal } from "../../shared/ui";
import { t, useLocale } from "../../shared/locale";
import { useWorkspace } from "../context";
import type { V4Asset } from "./types";

/** Source media only. This never presents a project frame or mix as a preview. */
export function V4ResourcePreview({
  asset,
  projectId,
  revision,
  onClose,
}: {
  asset: V4Asset;
  projectId: string;
  revision: string;
  onClose(): void;
}) {
  useLocale();
  const { api } = useWorkspace();
  const [url, setUrl] = useState<string | null>(null),
    [error, setError] = useState<string | null>(null);
  useEffect(() => {
    const abort = new AbortController();
    setUrl(null);
    setError(null);
    const load = async () => {
      if (!api) throw new Error("OpenCut is not ready.");
      const resource = await api.v4.resource(projectId, revision, asset.id);
      if (
        resource.project_id !== projectId ||
        resource.revision !== revision ||
        resource.resource_id !== asset.id ||
        resource.version.content_hash !== asset.version.content_hash ||
        resource.version.size !== asset.version.size ||
        resource.version.id !== asset.version.id
      )
        throw new Error("The selected media has changed. Refresh the project.");
      const source = api.sdk.projectV4ResourceUrl(resource);
      if (new URL(source).origin !== location.origin)
        throw new Error("Source preview requires the local workspace.");
      abort.signal.throwIfAborted();
      setUrl(source);
    };
    void load().catch((error) => {
      if (!abort.signal.aborted)
        setError(error instanceof Error ? error.message : String(error));
    });
    return () => abort.abort();
  }, [api, projectId, revision, asset]);
  return (
    <Modal title={t("Source media preview")} onClose={onClose}>
      <p>{asset.name}</p>
      <p className="muted">
        {t(
          "Shows the original source media. Scene edits, notes and audio mixing are not included.",
        )}
      </p>
      {error ? (
        <p role="alert">{t(error)}</p>
      ) : !url ? (
        <p role="status">{t("Loading source…")}</p>
      ) : asset.kind === "image" || asset.kind === "graphic" ? (
        <img className="source-preview" src={url} alt={asset.name} />
      ) : asset.kind === "audio" ? (
        <audio
          className="source-preview"
          src={url}
          controls
          preload="metadata"
        />
      ) : (
        <video
          className="source-preview"
          src={url}
          controls
          preload="metadata"
          playsInline
        />
      )}
    </Modal>
  );
}
