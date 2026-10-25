import { useEffect, useRef, useState } from "react";
import { CheckCircle2, Download, RefreshCw } from "lucide-react";
import type { ArtifactReference } from "../../../lib/editor-sdk";
import { Button } from "../../shared/ui";
import { t, useLocale } from "../../shared/locale";
import { saveResponse } from "../../shared/download";
import { friendlyError } from "../../shared/error-notice";
import { useWorkspace } from "../context";

/** A published reference is explicitly unchecked. Availability is observed via
 * the existing authenticated, hash-verifying byte port, never a guessed path.
 * Checking releases the stream; download rechecks authority and bytes again. */
export function ArtifactDownload({ artifact, name, label = "Download" }: { artifact: ArtifactReference; name: string; label?: string }) {
  useLocale();
  const { api } = useWorkspace();
  const [available, setAvailable] = useState(false), [pending, setPending] = useState<"check" | "download" | null>(null), [error, setError] = useState<unknown>(null);
  const generation = useRef(0), active = useRef<AbortController | null>(null);
  const fingerprint = JSON.stringify([artifact.job_id, artifact.artifact_id, artifact.content_hash, artifact.size_bytes, artifact.mime_type]);
  useEffect(() => {
    ++generation.current; setAvailable(false); setError(null); setPending(null);
    return () => { ++generation.current; active.current?.abort(); active.current = null; };
  }, [api, fingerprint]);
  const run = async (download: boolean) => {
    if (!api || active.current) return;
    const abort = new AbortController(), epoch = generation.current;
    active.current = abort; setPending(download ? "download" : "check"); setError(null);
    try {
      if (download) await saveResponse(name, () => api.sdk.downloadArtifact(artifact, { signal: abort.signal }));
      else {
        const response = await api.sdk.downloadArtifact(artifact, { signal: abort.signal });
        try {
          if (!response.ok || !response.body) throw Error("The published export file is unavailable.");
        } finally { await response.body?.cancel().catch(() => {}); }
      }
      if (epoch === generation.current) setAvailable(true);
    } catch (e) {
      if (epoch === generation.current && !(e instanceof DOMException && e.name === "AbortError")) { setAvailable(false); setError(e); }
    } finally {
      if (active.current === abort) active.current = null;
      if (epoch === generation.current) setPending(null);
    }
  };
  return <div className="export-file-action">
    <Button disabled={!api || !!pending} icon={available ? <Download size={14} /> : pending ? <RefreshCw size={14} /> : <CheckCircle2 size={14} />}
      title={t(available ? "Availability was checked. Download verifies the file again." : "Check the published file before downloading.")}
      onClick={() => void run(available)}>{t(pending === "download" ? "Saving file…" : pending ? "Checking file…" : available ? label : error ? "Check again" : "Check file")}</Button>
    {error !== null && <details className="export-file-error"><summary>{friendlyError(error)}</summary><pre>{error instanceof Error ? error.message : String(error)}</pre></details>}
  </div>;
}
