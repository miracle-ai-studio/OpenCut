import { useEffect, useRef, useState } from "react";
import { Button, Field, Panel } from "../../shared/ui";
import { t, useLocale } from "../../shared/locale";
import { authenticatedIdentity } from "../api";
import { useWorkspace } from "../context";
import { uploadMedia } from "../upload";
import { OpenCutApiError, type ToolOutput } from "../../../lib/editor-sdk";
import { CatalogRecoveryV1, type CatalogImportRequestV1, type CatalogGenerateRequestV1, type CatalogRequestV1 } from "./catalog_resources_v1-recovery";
import { filterCatalogV1, flagPngV1, type CatalogEntryV1 } from "./catalog_resources_v1";
import { V4GraphicActions } from "./graphic/workspace";
import { graphicKinds } from "./graphic/types";
import { PlaceMediaDialog, MediaPlanDialog } from "./media-dialog";
import { V4ResourcePreview } from "./resource-preview";
import type { V4Asset } from "./types";
import { V4BrandCatalogPanel } from "./brand_resources_v1-panel";

export function V4ResourceCatalogPanel() {
  useLocale();
  const { api, v4, busy, controller } = useWorkspace();
  const [catalog, setCatalog] = useState<ToolOutput<"opencut_catalog_resources_v1_list"> | null>(null), [category, setCategory] = useState<CatalogEntryV1["category"] | "brands">("sounds"), [query, setQuery] = useState(""), [limit, setLimit] = useState(40);
  const [pending, setPending] = useState(false), [error, setError] = useState<string | null>(null), [progress, setProgress] = useState<string | null>(null), [original, setOriginal] = useState<CatalogRequestV1 | null>(null);
  const [placing, setPlacing] = useState<V4Asset | null>(null), [review, setReview] = useState(false), [preview, setPreview] = useState<{asset: V4Asset; projectId: string; revision: string} | null>(null);
  const [recipe, setRecipe] = useState<CatalogGenerateRequestV1["recipe"]>({ kind: "whoosh", duration_ms: 750, seed: 1, level_db: -6 });
  const abort = useRef<AbortController | null>(null), active = useRef(false), recovery = useRef(new CatalogRecoveryV1()), epoch = useRef(0);
  const project = v4?.head.project.id;
  useEffect(() => {
    const run = ++epoch.current, cancel = new AbortController(); setCatalog(null); setError(null); setOriginal(null); setPreview(null); setPlacing(null); setReview(false);
    if (api && project) void (async () => {
      await api.authenticateConnection();
      const saved = recovery.current.get(authenticatedIdentity(api), project), data = await api.sdk.call("opencut_catalog_resources_v1_list", {}, { signal: cancel.signal });
      if (run !== epoch.current) return;
      if (data.schema_version !== 1) throw Error("Unsupported resource catalog."); setCatalog(data); setOriginal(saved);
    })().catch(e => { if (!cancel.signal.aborted && run === epoch.current) setError(e instanceof Error ? e.message : String(e)); });
    return () => { ++epoch.current; cancel.abort(); abort.current?.abort(); };
  }, [api, project]);
  useEffect(() => { setLimit(40); }, [query, category]);
  if (!api || !v4) return null;
  const disabled = busy || pending || !!v4.historical || !!v4.media.attempt || !!v4.recovery?.record || !!v4.recovery?.error;
  const current = () => { const s = controller.getSnapshot(); if (s.api !== api || !s.v4 || s.v4.head.project.id !== project || s.v4.historical) throw Error("Return to the original project before continuing."); return s.v4; };
  const execute = async (request: CatalogRequestV1, signal: AbortSignal, run: number) => {
    // Durably retain the exact typed request before the mutating dispatch.
    recovery.current.put(authenticatedIdentity(api), request); if (run === epoch.current) setOriginal(request);
    let receipt: ToolOutput<"opencut_catalog_resources_v1_import">;
    try { receipt = "recipe" in request ? await controller.v4.generateCatalogSound(request, signal) : await controller.v4.importCatalogResource(request, signal); }
    catch (error) {
      // begin() checks the original-key receipt before reporting conflict: this
      // response proves this exact command did not commit against that head.
      if (error instanceof OpenCutApiError && error.code === "REVISION_CONFLICT") { recovery.current.remove(authenticatedIdentity(api), request.project_id, request.idempotency_key); if (run === epoch.current) setOriginal(null); }
      throw error;
    }
    if (receipt.base_revision !== request.expected_revision || !receipt.created_ids.includes(request.asset_id)) throw Error("Catalog import returned another resource intent.");
    if (run !== epoch.current) return;
    await controller.v4.refresh();
    const state = current(), asset = state.head.project.assets.find(a => a.id === request.asset_id);
    if (!asset) throw Error("The acquired resource is not present in the refreshed project. Retry the original request.");
    recovery.current.remove(authenticatedIdentity(api), request.project_id, request.idempotency_key); setOriginal(null);
    setPreview({ asset, projectId: state.head.project.id, revision: state.head.revision });
  };
  const acquire = async (entry?: CatalogEntryV1, localRecipe?: CatalogGenerateRequestV1["recipe"]) => {
    if (active.current || disabled) return;
    const run = epoch.current, cancel = new AbortController(); abort.current = cancel; active.current = true; setPending(true); setError(null); setProgress(null);
    try {
      await api.authenticateConnection();
      const state = current(), saved = recovery.current.get(authenticatedIdentity(api), state.head.project.id);
      if (saved) { await execute(saved, cancel.signal, run); return; }
      if (localRecipe) {
        const request: CatalogGenerateRequestV1 = { project_id: state.head.project.id, expected_revision: state.head.revision, idempotency_key: crypto.randomUUID(), asset_id: crypto.randomUUID(), recipe: structuredClone(localRecipe) };
        await execute(request, cancel.signal, run); return;
      }
      if (!entry || entry.category === "shapes") throw Error("Choose a catalog resource.");
      if (entry.category === "sounds" && !catalog?.sound_download_available) throw Error(catalog?.sound_unavailable_reason ?? "Sound acquisition is unavailable.");
      const request: CatalogImportRequestV1 = { project_id: state.head.project.id, expected_revision: state.head.revision, idempotency_key: crypto.randomUUID(), catalog_id: entry.id, asset_id: crypto.randomUUID(), uploaded: null };
      if (entry.category === "flags") {
        const file = await flagPngV1(entry, cancel.signal);
        const imported = await uploadMedia(api.sdk, request.project_id, file, value => { if (run === epoch.current) setProgress(value); }, cancel.signal);
        if (!imported.version) throw Error("Flag upload has no immutable identity.");
        request.uploaded = { media_id: imported.media_id, version: imported.version };
        if (current().head.revision !== request.expected_revision) throw Error("Project changed. The completed flag upload remains in available media.");
      }
      cancel.signal.throwIfAborted(); await execute(request, cancel.signal, run);
    } catch (e) { if (run === epoch.current) { const message = e instanceof Error ? e.message : String(e); setError(message); controller.report(e); } }
    finally { active.current = false; setPending(false); if (run === epoch.current) setProgress(null); }
  };
  const entries = catalog && category !== "brands" ? filterCatalogV1(catalog.entries, category, query) : [];
  const acquired = v4.head.project.assets.filter(a => a.origin.kind === "online" && a.origin.provider === "kenney-ui-audio-v1" || a.origin.kind === "generated" && ["classic-v31-flag-browser-raster", "opencut-local-sfx-v1"].includes(a.origin.provider));
  const download = async (asset: V4Asset) => { try { const state = current(), resource = await api.v4.resource(state.head.project.id, state.head.revision, asset.id); if (resource.version.content_hash !== asset.version.content_hash) throw Error("Resource identity changed."); const url = api.sdk.projectV4ResourceUrl(resource); if (new URL(url).origin !== location.origin) throw Error("Resource download requires the local workspace."); const link = document.createElement("a"); link.href = url; link.download = `${asset.name}.${asset.kind === "audio" ? asset.metadata.mime_type === "audio/wav" ? "wav" : "ogg" : "png"}`; link.click(); } catch (e) { controller.report(e); } };
  return <Panel title={t("Resource catalog")} className="v4-resource-catalog">
    <Field label={t("Category")}><select disabled={pending} value={category} onChange={e => { const value = e.target.value; if (value === "sounds" || value === "flags" || value === "shapes" || value === "brands") setCategory(value); }}><option value="sounds">{t("Sounds")}</option><option value="flags">{t("Flags")}</option><option value="shapes">{t("Shapes")}</option><option value="brands">{t("Brands")}</option></select></Field>
    <Field label={t("Search catalog")}><input value={query} onChange={e => setQuery(e.target.value)} /></Field>
    {error && <p role="alert">{t(error)}</p>}{progress && <p role="status">{progress}</p>}
    {pending && <Button onClick={() => abort.current?.abort()}>{t("Stop waiting")}</Button>}
    {original && <div><p>{t("An acquisition is unresolved. Retry its original request before acquiring another resource.")}</p><Button disabled={disabled} onClick={() => void acquire()}>{t("Retry original acquisition")}</Button></div>}
    {category === "brands" ? <V4BrandCatalogPanel query={query} /> : category === "shapes" ? catalog && <V4GraphicActions allowedKinds={graphicKinds.filter(kind => entries.some(entry => entry.id === `shapes:${kind}`))} /> : <>
      {category === "sounds" && <p>{t("Kenney UI Audio · CC0. Acquire once to audition, download or place its original source.")}</p>}
      {category === "sounds" && <p>{t("Sounds use a local archive when available; downloading requires this host's permission. Sources are verified when acquired.")}</p>}
      {category === "sounds" && catalog?.sound_unavailable_reason && <p role="status">{t(catalog.sound_unavailable_reason)}</p>}
      {entries.slice(0, limit).map(entry => <article key={entry.id}><strong>{entry.name}</strong><span> · {entry.license.identifier}</span><a href={entry.source_url} target="_blank" rel="noreferrer">{t("Source")}</a><Button disabled={disabled || !!original || entry.category === "sounds" && !catalog?.sound_download_available} onClick={() => void acquire(entry)}>{t("Acquire and preview")}</Button></article>)}
      {entries.length > limit && <Button onClick={() => setLimit(value => value + 40)}>{t("Show more")}</Button>}
    </>}
    {category === "sounds" && <fieldset disabled={disabled || !!original}><legend>{t("Generate a local sound")}</legend>
      <p>{t("OpenCut local synthesis: recorded recipe and seed, no network or source recordings.")}</p>
      <Field label={t("Sound recipe")}><select value={recipe.kind} onChange={e => { const kind = e.target.value; if (kind === "whoosh" || kind === "click" || kind === "impact") setRecipe(r => ({ ...r, kind, duration_ms: kind === "click" ? 100 : kind === "impact" ? 600 : 750 })); }}><option value="whoosh">{t("Whoosh")}</option><option value="click">{t("Click")}</option><option value="impact">{t("Impact")}</option></select></Field>
      <Field label={t("Duration (ms)")}><input type="number" min={recipe.kind === "click" ? 40 : 100} max={recipe.kind === "click" ? 500 : recipe.kind === "impact" ? 3000 : 5000} step="1" value={recipe.duration_ms} onChange={e => setRecipe(r => ({ ...r, duration_ms: Number(e.target.value) }))} /></Field>
      <Field label={t("Seed")}><input type="number" min="0" max="4294967295" step="1" value={recipe.seed} onChange={e => setRecipe(r => ({ ...r, seed: Number(e.target.value) }))} /></Field>
      <Field label={t("Level (dB)")}><input type="number" min="-60" max="0" step="1" value={recipe.level_db} onChange={e => setRecipe(r => ({ ...r, level_db: Number(e.target.value) }))} /></Field>
      <Button onClick={() => void acquire(undefined, recipe)}>{t("Generate and preview")}</Button>
    </fieldset>}
    {acquired.length > 0 && <div><h3>{t("Acquired resources")}</h3>{acquired.map(asset => <article key={asset.id}><strong>{asset.name}</strong><Button onClick={() => setPreview({ asset, projectId: v4.head.project.id, revision: v4.head.revision })}>{t("Preview source")}</Button><Button onClick={() => void download(asset)}>{t("Download source")}</Button><Button disabled={disabled} onClick={() => setPlacing(asset)}>{t("Place media")}</Button></article>)}</div>}
    {preview && <V4ResourcePreview {...preview} onClose={() => setPreview(null)} />}
    {placing && <PlaceMediaDialog asset={placing} onClose={() => setPlacing(null)} onPlanned={() => { setPlacing(null); setReview(true); }} />}
    {review && <MediaPlanDialog onClose={() => setReview(false)} />}
  </Panel>;
}
