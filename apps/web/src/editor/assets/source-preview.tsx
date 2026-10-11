import { t, useLocale } from "../shared/locale";
import { useEffect, useState } from "react";
import type { Asset } from "../model/types";
import { useWorkspace } from "../workspace/context";
import { Modal } from "../shared/ui";
export function SourcePreview({
  asset,
  onClose,
}: {
  asset: Asset;
  onClose: () => void;
}) {
  useLocale();
  const { api, snapshot } = useWorkspace();
  const [url, setUrl] = useState<string | null>(null),
    [error, setError] = useState<string | null>(null);
  useEffect(() => {
    const abort = new AbortController();
    const load = async () => {
      if (!api || !snapshot) return;
      if (!asset.version || !asset.uri.startsWith("opencut-media://"))
        throw new Error("Reimport this media to preview it in the browser.");
      const resource = await api.sdk.call(
        "opencut_project_media_inspect",
        {
          project_id: snapshot.project.id,
          media_id: asset.uri.slice("opencut-media://".length),
        },
        { signal: abort.signal },
      );
      if (resource.asset.version?.content_hash !== asset.version.content_hash)
        throw new Error("The selected media has changed. Refresh the project.");
      const sourceUrl = api.sdk.projectMediaUrl(resource);
      if (new URL(sourceUrl).origin !== location.origin)
        throw new Error("Source preview requires the local workspace.");
      abort.signal.throwIfAborted();
      setUrl(sourceUrl);
    };
    void load().catch((error) => {
      if (!abort.signal.aborted) setError(String(error));
    });
    return () => {
      abort.abort();
    };
  }, [api, snapshot?.project.id, asset]);
  return (
    <Modal title={asset.name} onClose={onClose}>
      {error ? (
        <p role="alert" className="error">
          {error}
        </p>
      ) : !url ? (
        <p role="status">{t("Loading source…")}</p>
      ) : asset.kind === "image" ? (
        <img className="source-preview" src={url} alt={asset.name} />
      ) : asset.kind === "audio" ? (
        <audio
          className="source-preview"
          src={url}
          preload="metadata"
          controls
        />
      ) : (
        <video
          className="source-preview"
          src={url}
          preload="metadata"
          controls
          playsInline
        />
      )}
    </Modal>
  );
}
