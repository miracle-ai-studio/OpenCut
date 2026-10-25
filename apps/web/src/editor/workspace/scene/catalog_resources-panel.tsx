import { useResourceDrag } from "./workbench/resource-drag";
import type { ResourcePosition } from "./workbench/resource-drop-model";
import { Select } from "../../shared/controls";
import { useEffect, useRef, useState } from "react";
import { Info, Plus,Flag,Shapes } from "lucide-react";
import { Button, Field, Panel } from "../../shared/ui";
import { t, useLocale } from "../../shared/locale";
import { authenticatedIdentity } from "../api";
import { useWorkspace } from "../context";
import { uploadMedia } from "../upload";
import { OpenCutApiError, type ToolOutput } from "../../../lib/editor-sdk";
import {
  CatalogRecovery,
  type CatalogImportRequest,
  type CatalogGenerateRequest,
  type CatalogRequest,
} from "./catalog_resources-recovery";
import { filterCatalog, flagPng, type CatalogEntry } from "./catalog_resources";
import { GraphicActions } from "./graphic/workspace";
import { graphicKinds } from "./graphic/types";
import { placeMediaDirect, MediaPlanDialog } from "./media-dialog";
import { ResourcePreview } from "./resource-preview";
import { CatalogSoundPreview } from "./catalog-sound-preview";
import type { Asset } from "./types";
import { BrandCatalogPanel } from "./brand_resources-panel";
import { soundName, useResourceCopy } from "./workbench/resources-copy";
import "./workbench/resources.css";

export function ResourceCatalogPanel({
  category: chosenCategory,
  active: visible = true,
}: { category?: CatalogEntry["category"] | "brands"; active?: boolean } = {}) {
  const resourceDrag = useResourceDrag();
  const { language } = useLocale(),
    c = useResourceCopy();
  const { api, scene, busy, controller } = useWorkspace();
  const [catalog, setCatalog] =
      useState<ToolOutput<"opencut_catalog_resources_list"> | null>(null),
    [localCategory, setCategory] = useState<
      CatalogEntry["category"] | "brands"
    >("sounds"),
    [queries, setQueries] = useState<Partial<Record<CatalogEntry["category"] | "brands", string>>>({}),
    [limits, setLimits] = useState<Partial<Record<CatalogEntry["category"] | "brands", number>>>({});
  const category = chosenCategory ?? localCategory;
  const query = queries[category] ?? "", limit = limits[category] ?? 40;
  const [pending, setPending] = useState(false),
    [error, setError] = useState<string | null>(null),
    [progress, setProgress] = useState<string | null>(null),
    [original, setOriginal] = useState<CatalogRequest | null>(null);
  const [review, setReview] = useState(false),
    [notice, setNotice] = useState<"saved" | "addedProject" | null>(null),
    [listening, setListening] = useState<string | null>(null),
    [preview, setPreview] = useState<{
      asset: Asset;
      projectId: string;
      revision: string;
    } | null>(null);
  const [recipe, setRecipe] = useState<CatalogGenerateRequest["recipe"]>({
    kind: "whoosh",
    duration_ms: 750,
    seed: 1,
    level_db: -6,
  });
  const abort = useRef<AbortController | null>(null),
    dragOrigin = useRef<Element | null>(null),
    soundList = useRef<HTMLDivElement | null>(null),
    active = useRef(false),
    recovery = useRef(new CatalogRecovery()),
    epoch = useRef(0);
  const project = scene?.head.project.id;
  useEffect(() => {
    const close = (event: PointerEvent) => {
      for (const details of soundList.current?.querySelectorAll<HTMLDetailsElement>(":scope > article > details[open]") ?? [])
        if (event.target instanceof Node && !details.contains(event.target)) details.open = false;
    };
    document.addEventListener("pointerdown", close);
    return () => document.removeEventListener("pointerdown", close);
  }, []);
  useEffect(() => {
    const run = ++epoch.current,
      cancel = new AbortController();
    active.current = false;
    setCatalog(null);
    setError(null);
    setOriginal(null);
    setPreview(null);
    setReview(false);
    setListening(null);
    setNotice(null);
    setPending(false);
    if (api && project)
      void (async () => {
        await api.authenticateConnection();
        const saved = recovery.current.get(authenticatedIdentity(api), project),
          data = await api.sdk.call(
            "opencut_catalog_resources_list",
            {},
            { signal: cancel.signal },
          );
        if (run !== epoch.current) return;
        setCatalog(data);
        setOriginal(saved);
      })().catch((e) => {
        if (!cancel.signal.aborted && run === epoch.current)
          setError(e instanceof Error ? e.message : String(e));
      });
    return () => {
      ++epoch.current;
      cancel.abort();
      abort.current?.abort();
    };
  }, [api, api?.connectionIdentity, project]);
  useEffect(() => {
    setListening(null);
  }, [query, category]);
  useEffect(() => {
    if (!visible) {
      setListening(null);
      setPreview(null);
    }
  }, [visible]);
  if (!api || !scene) return null;
  const disabled =
    !!scene.propertyDrafts?.length ||
    busy ||
    pending ||
    !!scene.historical ||
    !!scene.media.attempt ||
    !!scene.recovery?.record ||
    !!scene.recovery?.error;
  const current = () => {
    const s = controller.getSnapshot();
    if (
      s.api !== api ||
      !s.scene ||
      s.scene.head.project.id !== project ||
      s.scene.historical
    )
      throw Error("Return to the original project before continuing.");
    return s.scene;
  };
  const execute = async (
    request: CatalogRequest,
    signal: AbortSignal,
    run: number,
  ) => {
    const identity = authenticatedIdentity(api);
    // Keep the exact request before dispatch: a timeout never authorizes a new import.
    recovery.current.put(identity, request);
    if (run === epoch.current) setOriginal(request);
    let receipt: ToolOutput<"opencut_catalog_resources_import">;
    try {
      receipt =
        "recipe" in request
          ? await controller.scene.generateCatalogSound(request, signal)
          : await controller.scene.importCatalogResource(request, signal);
    } catch (error) {
      if (
        error instanceof OpenCutApiError &&
        error.code === "REVISION_CONFLICT"
      ) {
        recovery.current.remove(
          identity,
          request.project_id,
          request.idempotency_key,
        );
        if (run === epoch.current) setOriginal(null);
      }
      throw error;
    }
    if (
      receipt.base_revision !== request.expected_revision ||
      !receipt.created_ids.includes(request.asset_id)
    )
      throw Error("Catalog import returned another resource intent.");
    if (run !== epoch.current || authenticatedIdentity(api) !== identity)
      throw new DOMException("Project changed", "AbortError");
    await controller.scene.refresh();
    if (run !== epoch.current || authenticatedIdentity(api) !== identity)
      throw new DOMException("Project changed", "AbortError");
    const state = current(),
      asset = state.head.project.assets.find((a) => a.id === request.asset_id);
    if (!asset)
      throw Error(
        "The acquired resource is not present in the refreshed project. Retry the original request.",
      );
    recovery.current.remove(
      identity,
      request.project_id,
      request.idempotency_key,
    );
    setOriginal(null);
    return { asset, revision: receipt.revision };
  };
  const acquire = async (
    entry?: CatalogEntry,
    localRecipe?: CatalogGenerateRequest["recipe"],
    dropPosition?: ResourcePosition,
  ) => {
    if (active.current || disabled) {
      if (dropPosition) throw Error("Finish or cancel the current edit first.");
      return;
    }
    const run = epoch.current,
      cancel = new AbortController();
    abort.current = cancel;
    active.current = true;
    setPending(true);
    setError(null);
    setProgress(null);
    setNotice(null);
    try {
      await api.authenticateConnection();
      if (run !== epoch.current) return;
      const state = current(),
        identity = authenticatedIdentity(api),
        saved = recovery.current.get(identity, state.head.project.id);
      if (dropPosition && (state.head.project.id !== dropPosition.projectId ||
        state.head.revision !== dropPosition.revision || state.sequenceId !== dropPosition.sequenceId))
        throw Error("The drop target changed.");
      if (dropPosition && saved) throw Error("Finish the original resource acquisition before dragging another resource.");
      // A recovered acquisition finishes the original import only. Never append a
      // fresh timeline command after a page reload with no retained placement intent.
      if (saved) {
        await execute(saved, cancel.signal, run);
        if (run === epoch.current) setNotice("addedProject");
        return;
      }
      const position = dropPosition ?? {
        projectId: state.head.project.id,
        sequenceId: state.sequenceId,
        at: structuredClone(state.playhead),
      };
      let request: CatalogRequest;
      if (localRecipe)
        request = {
          project_id: state.head.project.id,
          expected_revision: state.head.revision,
          idempotency_key: crypto.randomUUID(),
          asset_id: crypto.randomUUID(),
          recipe: structuredClone(localRecipe),
        };
      else {
        if (!entry || entry.category === "shapes")
          throw Error("Choose a catalog resource.");
        if (entry.category === "sounds" && !catalog?.sound_download_available)
          throw Error(
            catalog?.sound_unavailable_reason ??
              "Sound acquisition is unavailable.",
          );
        const importedRequest: CatalogImportRequest = {
          project_id: state.head.project.id,
          expected_revision: state.head.revision,
          idempotency_key: crypto.randomUUID(),
          catalog_id: entry.id,
          asset_id: crypto.randomUUID(),
          uploaded: null,
        };
        if (entry.category === "flags") {
          const file = await flagPng(entry, cancel.signal);
          const imported = await uploadMedia(
            api.sdk,
            importedRequest.project_id,
            file,
            (value) => {
              if (run === epoch.current) setProgress(value);
            },
            cancel.signal,
          );
          if (!imported.version)
            throw Error("Flag upload has no immutable identity.");
          importedRequest.uploaded = {
            media_id: imported.media_id,
            version: imported.version,
          };
          if (current().head.revision !== importedRequest.expected_revision)
            throw Error(
              "Project changed. The completed flag upload remains in available media.",
            );
        }
        request = importedRequest;
      }
      cancel.signal.throwIfAborted();
      const result = await execute(request, cancel.signal, run);
      if (run !== epoch.current || identity !== authenticatedIdentity(api))
        return;
      cancel.signal.throwIfAborted();
      const placement = await placeMediaDirect(controller, result.asset, {
        ...position,
        revision: result.revision,
      });
      if (run === epoch.current) {
        setNotice("saved");
        setReview(!!controller.getSnapshot().scene?.media.attempt);
      }
      return placement;
    } catch (e) {
      if (
        run === epoch.current &&
        controller.getSnapshot().api === api &&
        controller.getSnapshot().scene?.head.project.id === project
      ) {
        setError(e instanceof Error ? e.message : String(e));
        controller.report(e);
        setReview(!!controller.getSnapshot().scene?.media.attempt);
      }
      if (dropPosition) throw e;
    } finally {
      if (run === epoch.current) {
        active.current = false;
        setPending(false);
        setProgress(null);
      }
    }
  };
  const entries =
    catalog && category !== "brands"
      ? filterCatalog(catalog.entries, category, query, (name) =>
          category === "sounds"
            ? soundName(name, language)
            : t(
                category === "shapes"
                  ? name[0].toUpperCase() + name.slice(1)
                  : name,
              ),
        )
      : [];
  const acquired = scene.head.project.assets.filter((asset) =>
    category === "sounds"
      ? asset.kind === "audio" &&
        ((asset.origin.kind === "online" &&
          asset.origin.provider === "kenney-ui-audio-v1") ||
          (asset.origin.kind === "generated" &&
            asset.origin.provider === "opencut-local-sfx-v1"))
      : category === "flags" &&
        asset.origin.kind === "generated" &&
        asset.origin.provider === "classic-v31-flag-browser-raster",
  );
  const download = async (asset: Asset) => {
    try {
      const state = current(),
        identity = authenticatedIdentity(api),
        revision = state.head.revision;
      const resource = await api.scene.resource(
        state.head.project.id,
        revision,
        asset.id,
      );
      if (
        controller.getSnapshot().api !== api ||
        authenticatedIdentity(api) !== identity ||
        current().head.revision !== revision ||
        resource.project_id !== state.head.project.id ||
        resource.revision !== revision ||
        resource.resource_id !== asset.id ||
        resource.version.content_hash !== asset.version.content_hash ||
        resource.version.id !== asset.version.id ||
        resource.version.size !== asset.version.size
      )
        throw Error("Resource identity changed.");
      const url = api.sdk.projectResourceUrl(resource);
      if (new URL(url).origin !== location.origin)
        throw Error("Resource download requires the local workspace.");
      const link = document.createElement("a");
      link.href = url;
      link.download = `${asset.name}.${asset.kind === "audio" ? (asset.metadata.mime_type === "audio/wav" ? "wav" : "ogg") : "png"}`;
      link.click();
    } catch (error) {
      controller.report(error);
    }
  };
  const addExisting = async (asset: Asset) => {
    try {
      await placeMediaDirect(controller, asset);
      setNotice("saved");
    } catch (error) {
      controller.report(error);
    } finally {
      if (
        controller.getSnapshot().api === api &&
        controller.getSnapshot().scene?.head.project.id === project
      )
        setReview(!!controller.getSnapshot().scene?.media.attempt);
    }
  };
  return (
    <Panel
      title={t(category === "sounds" ? "Sound library" : "Resource catalog")}
      className="v4-resource-catalog"
    >
      {!chosenCategory && (
        <Field label={t("Category")}>
          <Select
            disabled={pending}
            value={category}
            onChange={(e) => {
              const value = e.target.value;
              if (
                value === "sounds" ||
                value === "flags" ||
                value === "shapes" ||
                value === "brands"
              )
                setCategory(value);
            }}
          >
            <option value="sounds">{t("Sounds")}</option>
            <option value="flags">{t("Flags")}</option>
            <option value="shapes">{t("Shapes")}</option>
            <option value="brands">{t("Brands")}</option>
          </Select>
        </Field>
      )}
      <Field label={t("Search catalog")}>
        <input value={query} onChange={(e) => {
          setQueries(values => ({...values, [category]: e.target.value}));
          setLimits(values => ({...values, [category]: 40}));
        }} />
      </Field>
      {error && <p role="alert">{t(error)}</p>}
      {progress && <p role="status">{progress}</p>}
      {notice && <p role="status">{c(notice)}</p>}
      {pending && (
        <Button onClick={() => abort.current?.abort()}>
          {t("Stop waiting")}
        </Button>
      )}
      {original && (
        <div className="resource-recovery">
          <p>
            {t(
              "An acquisition is unresolved. Retry its original request before acquiring another resource.",
            )}
          </p>
          <Button disabled={disabled} onClick={() => void acquire()}>
            {c("retry")}
          </Button>
        </div>
      )}
      {category === "brands" ? (
        <BrandCatalogPanel query={query} />
      ) : category === "shapes" ? (
        catalog && (
          <GraphicActions
            cards
            allowedKinds={graphicKinds.filter((kind) =>
              entries.some((entry) => entry.id === `shapes:${kind}`),
            )}
          />
        )
      ) : (
        <>
          {category === "sounds" && <p className="muted">{c("libraryHint")}</p>}
          {category === "sounds" && catalog?.sound_unavailable_reason && (
            <p role="status">{t(catalog.sound_unavailable_reason)}</p>
          )}
          {catalog && !entries.length && category === "sounds" && (
            <p>{c("noMatches")}</p>
          )}
          <div className="resource-sound-list" ref={soundList}>
            {entries.slice(0, limit).map((entry) => (
              <article className="resource-sound-row" data-resource-category={entry.category} key={entry.id}
                tabIndex={entry.category === "sounds" ? 0 : undefined}
                data-shortcuts-ignore={entry.category === "sounds" ? true : undefined}
                aria-label={entry.category === "sounds" ? soundName(entry.name, language) : undefined}
                onPointerDownCapture={event => { dragOrigin.current = event.target instanceof Element ? event.target : null; }}
                onMouseDownCapture={event => { dragOrigin.current = event.target instanceof Element ? event.target : null; }}
                onPointerUpCapture={() => { dragOrigin.current = null; }}
                onKeyDown={event => {
                  if (entry.category !== "sounds") return;
                  const target = event.target instanceof Element ? event.target : null;
                  if (event.key === "Escape") {
                    const details = target?.closest("details");
                    if (details?.open) { details.open = false; details.querySelector("summary")?.focus(); }
                    else if (listening === entry.id) setListening(null);
                    else return;
                    event.preventDefault(); event.stopPropagation(); return;
                  }
                  if (event.key !== "Enter" || event.repeat || !(target === event.currentTarget || target?.closest(".resource-catalog-listen"))) return;
                  event.preventDefault(); event.stopPropagation();
                  if (event.shiftKey) { if (!disabled && !original && catalog?.sound_download_available) void acquire(entry); }
                  else event.currentTarget.querySelector<HTMLButtonElement>(".resource-catalog-listen")?.click();
                }}
                draggable={!!resourceDrag && !disabled && !resourceDrag.disabled && !original && entry.category === "sounds" && !!catalog?.sound_download_available}
                onDragStart={event => {
                  const origin = dragOrigin.current ?? (event.target instanceof Element ? event.target : null);
                  if (origin?.closest("button,a,summary,input,textarea,select,details,[role=slider]") || entry.category !== "sounds" || disabled || original || !catalog?.sound_download_available) { event.preventDefault(); return; }
                  resourceDrag?.start(event, {kind:"catalog-audio",name:soundName(entry.name,language),place:position=>acquire(entry,undefined,position)});
                }} onDragEnd={() => resourceDrag?.cancel()}>
                {entry.category === "sounds" ? <CatalogSoundPreview entry={entry} name={soundName(entry.name, language)} active={visible && listening === entry.id} disabled={!catalog?.sound_download_available} onActivate={() => setListening(entry.id)} onClose={() => setListening(null)} /> : <div className="resource-catalog-cover" aria-hidden="true">{entry.category === "flags" ? <Flag size={32}/> : <Shapes size={32}/>}</div>}
                <strong className="resource-sound-title" title={entry.category === "sounds" ? soundName(entry.name, language) : entry.name}>
                  {entry.category === "sounds"
                    ? soundName(entry.name, language)
                    : entry.name}
                </strong>
                <div className="resource-sound-controls">
                  <Button className="resource-catalog-add" aria-label={`${c("add")} · ${entry.category==="sounds"?soundName(entry.name,language):entry.name}`} title={c("add")}
                    icon={<Plus size={13} />}
                    disabled={
                      disabled ||
                      !!original ||
                      (entry.category === "sounds" &&
                        !catalog?.sound_download_available)
                    }
                    onClick={() => void acquire(entry)}
                  >
                    {c("add")}
                  </Button>
                </div>
                <details>
                  <summary aria-label={`${c("sources")} · ${entry.name}`} title={c("sources")}>{entry.category === "sounds" ? <Info size={14} /> : c("sources")}</summary>
                  <div className="resource-sound-license"><strong>{entry.license.identifier}</strong><p>{entry.license.attribution}</p><pre>{entry.license.notice}</pre>
                  <a href={entry.source_url} target="_blank" rel="noreferrer">
                    {t("Source")}
                  </a></div>
                </details>
              </article>
            ))}
          </div>
          {entries.length > limit && (
            <Button onClick={() => setLimits(values => ({...values, [category]: limit + 40}))}>
              {t("Show more")}
            </Button>
          )}
        </>
      )}
      {category === "sounds" && (
        <details className="resource-details">
          <summary>{c("createSound")}</summary>
          <fieldset disabled={disabled || !!original}>
            <legend>{c("soundOptions")}</legend>
            <p>{c("createHint")}</p>
            <Field label={t("Sound recipe")}>
              <Select
                value={recipe.kind}
                onChange={(e) => {
                  const kind = e.target.value;
                  if (
                    kind === "whoosh" ||
                    kind === "click" ||
                    kind === "impact"
                  )
                    setRecipe((r) => ({
                      ...r,
                      kind,
                      duration_ms:
                        kind === "click" ? 100 : kind === "impact" ? 600 : 750,
                    }));
                }}
              >
                <option value="whoosh">{t("Whoosh")}</option>
                <option value="click">{t("Click")}</option>
                <option value="impact">{t("Impact")}</option>
              </Select>
            </Field>
            <Field label={t("Duration (ms)")}>
              <input
                type="number"
                min={recipe.kind === "click" ? 40 : 100}
                max={
                  recipe.kind === "click"
                    ? 500
                    : recipe.kind === "impact"
                      ? 3000
                      : 5000
                }
                step="1"
                value={recipe.duration_ms}
                onChange={(e) =>
                  setRecipe((r) => ({
                    ...r,
                    duration_ms: Number(e.target.value),
                  }))
                }
              />
            </Field>
            <Field label={c("variation")}>
              <input
                type="number"
                min="0"
                max="4294967295"
                step="1"
                value={recipe.seed}
                onChange={(e) =>
                  setRecipe((r) => ({ ...r, seed: Number(e.target.value) }))
                }
              />
            </Field>
            <Field label={t("Level (dB)")}>
              <input
                type="number"
                min="-60"
                max="0"
                step="1"
                value={recipe.level_db}
                onChange={(e) =>
                  setRecipe((r) => ({ ...r, level_db: Number(e.target.value) }))
                }
              />
            </Field>
            <Button onClick={() => void acquire(undefined, recipe)}>
              {c("createAdd")}
            </Button>
          </fieldset>
        </details>
      )}
      {acquired.length > 0 && (
        <div>
          <h3>
            {c(category === "sounds" ? "projectSounds" : "projectGraphics")}
          </h3>
          {acquired.map((asset) => (
            <article className="resource-sound-row" key={asset.id}
              draggable={!!resourceDrag && !disabled && !resourceDrag.disabled}
              onDragStart={event=>resourceDrag?.start(event,{kind:"media",assetId:asset.id,name:asset.name})}
              onDragEnd={()=>resourceDrag?.cancel()}>
              <strong className="resource-sound-title" title={soundName(asset.name, language)}>
                {soundName(asset.name, language)}
              </strong>
              <Button
                onClick={() =>
                  setPreview({
                    asset,
                    projectId: scene.head.project.id,
                    revision: scene.head.revision,
                  })
                }
              >
                {t("Preview source")}
              </Button>
              <Button onClick={() => void download(asset)}>
                {t("Download source")}
              </Button>
              <Button
                disabled={disabled}
                onClick={() => void addExisting(asset)}
              >
                {c("add")}
              </Button>
            </article>
          ))}
        </div>
      )}
      {preview && (
        <ResourcePreview {...preview} onClose={() => setPreview(null)} />
      )}
      {review && <MediaPlanDialog onClose={() => setReview(false)} />}
    </Panel>
  );
}
