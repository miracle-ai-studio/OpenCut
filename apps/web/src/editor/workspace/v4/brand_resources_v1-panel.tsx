import { useEffect, useRef, useState } from "react";
import { Button } from "../../shared/ui";
import { t, useLocale } from "../../shared/locale";
import { OpenCutApiError, type ToolOutput } from "../../../lib/editor-sdk";
import { authenticatedIdentity } from "../api";
import { useWorkspace } from "../context";
import { uploadMedia } from "../upload";
import { BrandRecoveryV1 } from "./brand_resources_v1-recovery";
import { brandPngV1, brandProviderV1, type BrandEntryV1, type BrandImportV1 } from "./brand_resources_v1";
import { PlaceMediaDialog, MediaPlanDialog } from "./media-dialog";
import { V4ResourcePreview } from "./resource-preview";
import type { V4Asset } from "./types";

export function V4BrandCatalogPanel({ query }: { query: string }) {
  useLocale();
  const { api, v4, busy, controller } = useWorkspace();
  const [catalog, setCatalog] = useState<ToolOutput<"opencut_brand_resources_v1_list"> | null>(null), [offset, setOffset] = useState(0), [reading, setReading] = useState(false);
  const [original, setOriginal] = useState<BrandImportV1 | null>(null), [error, setError] = useState<string | null>(null), [pending, setPending] = useState(false), [progress, setProgress] = useState<string | null>(null);
  const [placing, setPlacing] = useState<V4Asset | null>(null), [review, setReview] = useState(false);
  const [preview, setPreview] = useState<{ asset: V4Asset; projectId: string; revision: string } | null>(null);
  const recovery = useRef(new BrandRecoveryV1()), context = useRef(0), active = useRef(false), abort = useRef<AbortController | null>(null);
  const project = v4?.head.project.id;
  useEffect(() => {
    ++context.current; setOriginal(null); setPlacing(null); setPreview(null); setReview(false); setError(null);
    const run = context.current;
    if (api && project) void api.authenticateConnection().then(() => { if (run === context.current) setOriginal(recovery.current.get(authenticatedIdentity(api), project)); }).catch(e => { if (run === context.current) setError(e instanceof Error ? e.message : String(e)); });
    return () => { ++context.current; abort.current?.abort(); };
  }, [api, project]);
  useEffect(() => { setOffset(0); }, [query]);
  useEffect(() => {
    const cancel = new AbortController(); setReading(true); setCatalog(null);
    if (api && project) void api.sdk.call("opencut_brand_resources_v1_list", { query: query.trim() || null, offset, limit: 40 }, { signal: cancel.signal }).then(value => {
      if (cancel.signal.aborted) return;
      if (value.schema_version !== 1) throw Error("Unsupported brand catalog."); setCatalog(value);
    }).catch(e => { if (!cancel.signal.aborted) setError(e instanceof Error ? e.message : String(e)); }).finally(() => { if (!cancel.signal.aborted) setReading(false); });
    return () => cancel.abort();
  }, [api, project, query, offset]);
  if (!api || !v4) return null;
  const disabled = busy || pending || !!v4.historical || !!v4.media.attempt || !!v4.recovery?.record || !!v4.recovery?.error;
  const current = (identity?: string) => { const state = controller.getSnapshot(); if (state.api !== api || identity !== undefined && api.connectionIdentity !== identity || !state.v4 || state.v4.historical || state.v4.head.project.id !== project) throw Error("Return to the original project and connection before continuing."); return state.v4; };
  const execute = async (request: BrandImportV1, signal: AbortSignal, run: number, identity: string) => {
    signal.throwIfAborted(); current(identity); if (run !== context.current) return;
    recovery.current.put(identity, request); setOriginal(request);
    let receipt: ToolOutput<"opencut_brand_resources_v1_import">;
    try { receipt = await controller.v4.importBrandResource(request, signal); }
    catch (e) { if (e instanceof OpenCutApiError && e.code === "REVISION_CONFLICT") { recovery.current.remove(identity, request.project_id, request.idempotency_key); if (run === context.current && api.connectionIdentity === identity) setOriginal(null); } throw e; }
    if (receipt.base_revision !== request.expected_revision || !receipt.created_ids.includes(request.asset_id)) throw Error("Brand receipt differs from the original resource intent.");
    if (run !== context.current || api.connectionIdentity !== identity) return;
    await controller.v4.refresh();
    if (run !== context.current || api.connectionIdentity !== identity) return;
    if (!current(identity).head.project.assets.some(asset => asset.id === request.asset_id)) throw Error("Acquired brand is absent from the refreshed project. Retry its original request.");
    recovery.current.remove(identity, request.project_id, request.idempotency_key); setOriginal(null);
  };
  const acquire = async (entry?: BrandEntryV1) => {
    if (active.current || disabled) return;
    const run = context.current, cancel = new AbortController(); abort.current = cancel; active.current = true; setPending(true); setError(null);
    try {
      await api.authenticateConnection();
      cancel.signal.throwIfAborted(); if (run !== context.current) return;
      const identity = authenticatedIdentity(api), state = current(identity), saved = recovery.current.get(identity, state.head.project.id);
      if (saved) { await execute(saved, cancel.signal, run, identity); return; }
      if (!entry?.import_available) throw Error(entry?.unavailable_reason ?? "Choose an available brand.");
      const source = await api.sdk.call("opencut_brand_resources_v1_source", { catalog_id: entry.id }, { signal: cancel.signal });
      cancel.signal.throwIfAborted(); current(identity); if (run !== context.current) return;
      const png = await brandPngV1(source, entry, cancel.signal);
      const uploaded = await uploadMedia(api.sdk, state.head.project.id, png, message => { if (run === context.current) setProgress(message); }, cancel.signal);
      if (!uploaded.version) throw Error("Brand upload has no immutable identity.");
      if (run !== context.current || current(identity).head.revision !== state.head.revision) throw Error("Project changed. The completed brand upload remains in available media.");
      cancel.signal.throwIfAborted();
      await execute({ schema_version: 1, project_id: state.head.project.id, expected_revision: state.head.revision, idempotency_key: crypto.randomUUID(), asset_id: crypto.randomUUID(), catalog_id: entry.id, uploaded: { media_id: uploaded.media_id, version: uploaded.version } }, cancel.signal, run, identity);
    } catch (e) { if (run === context.current) { setError(e instanceof Error ? e.message : String(e)); controller.report(e); } }
    finally { active.current = false; setPending(false); if (run === context.current) setProgress(null); }
  };
  const acquired = v4.head.project.assets.filter(asset => asset.origin.kind === "generated" && asset.origin.provider === brandProviderV1);
  const download = async (asset: V4Asset, license: boolean) => {
    try {
      if (license) {
        const url = URL.createObjectURL(new Blob([`${asset.license?.attribution ?? ""}\n\n${asset.license?.notice ?? ""}`], { type: "text/plain;charset=utf-8" }));
        try { const link = document.createElement("a"); link.href = url; link.download = `${asset.name}-LICENSE.txt`; link.click(); } finally { setTimeout(() => URL.revokeObjectURL(url), 0); }
      } else {
        const state = current(), resource = await api.v4.resource(state.head.project.id, state.head.revision, asset.id);
        if (resource.version.content_hash !== asset.version.content_hash) throw Error("Brand resource identity changed.");
        const url = api.sdk.projectV4ResourceUrl(resource); if (new URL(url).origin !== location.origin) throw Error("Brand download requires the local workspace.");
        const link = document.createElement("a"); link.href = url; link.download = `${asset.name}.png`; link.click();
      }
    } catch (e) { controller.report(e); }
  };
  return <section aria-label={t("Brands")}>
    <p>{t("Simple Icons · pinned offline collection. Each brand keeps its own license; trademarks and brand guidelines still apply.")}</p>
    {catalog && <p>{catalog.collection.release} · {catalog.matched} / {catalog.total} {t("brands")}</p>}
    {error && <p role="alert">{t(error)}</p>}{progress && <p role="status">{progress}</p>}
    {pending && <Button onClick={() => abort.current?.abort()}>{t("Stop waiting")}</Button>}
    {original && <div><p>{t("A brand acquisition is unresolved. Retry its original request before acquiring another brand.")}</p><Button disabled={disabled} onClick={() => void acquire()}>{t("Retry original brand acquisition")}</Button></div>}
    {reading && <p role="status">{t("Reading brand catalog…")}</p>}
    {catalog?.entries.length === 0 && <p role="status">{t("No matching brands.")}</p>}
    {catalog?.entries.map(entry => <div key={entry.id}>
      <strong>{entry.name}</strong> <small>{entry.license_identifier ?? t("License not recorded")}</small>
      <p><a href={entry.source_url} target="_blank" rel="noreferrer">{t("Original source")}</a>{entry.guidelines_url && <> · <a href={entry.guidelines_url} target="_blank" rel="noreferrer">{t("Brand guidelines")}</a></>}{entry.license_url && <> · <a href={entry.license_url} target="_blank" rel="noreferrer">{t("License")}</a></>}</p>
      {!entry.import_available && <p>{entry.unavailable_reason}</p>}
      <Button disabled={disabled || !!original || !entry.import_available} onClick={() => void acquire(entry)}>{t("Acquire brand")} · {entry.name}</Button>
    </div>)}
    {catalog && <div><Button disabled={reading || offset === 0} onClick={() => setOffset(Math.max(0, offset - 40))}>{t("Previous")}</Button><Button disabled={reading || catalog.next_offset == null} onClick={() => { if (catalog.next_offset != null) setOffset(catalog.next_offset); }}>{t("Next")}</Button></div>}
    {acquired.map(asset => <div key={asset.id}>
      <strong>{asset.name}</strong> <small>{asset.license?.identifier}</small>
      <Button onClick={() => setPreview({ asset, projectId: v4.head.project.id, revision: v4.head.revision })}>{t("Preview source")}</Button>
      <Button disabled={disabled} onClick={() => setPlacing(asset)}>{t("Place")}</Button>
      <Button onClick={() => void download(asset, false)}>{t("Download PNG")}</Button><Button onClick={() => void download(asset, true)}>{t("Download license and attribution")}</Button>
    </div>)}
    {preview && <V4ResourcePreview {...preview} onClose={() => setPreview(null)} />}
    {placing && <PlaceMediaDialog asset={placing} onClose={() => setPlacing(null)} onPlanned={() => { setPlacing(null); setReview(true); }} />}
    {review && <MediaPlanDialog onClose={() => setReview(false)} />}
  </section>;
}
