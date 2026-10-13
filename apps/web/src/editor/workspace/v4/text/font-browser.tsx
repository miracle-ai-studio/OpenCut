import { useEffect, useMemo, useRef, useState } from "react";
import type { ToolOutput } from "../../../../lib/editor-sdk";
import { Button, Field } from "../../../shared/ui";
import { t, useLocale } from "../../../shared/locale";
import type { V4FontImportProps } from "./font-import";
import "./font-browser.css";
import { mergeDerivedFontCatalog, derivedSourceMetadata } from "./font-derived";
import type { FontSourceMetadata } from "./font-derived";
import { FontAtlasPreview } from "./font-atlas-preview";

export type FontBrowserCatalog = ToolOutput<"opencut_font_v2_catalog">;
export type FontBrowserSource = ToolOutput<"opencut_font_v2_source">;
export type FontSourceReader = (
  catalogId: string,
  signal: AbortSignal,
) => Promise<FontBrowserSource>;
type Family = FontBrowserCatalog["families"][number];
type Face = Family["faces"][number];

export const fontFaceUnavailableMessage = (face: Face): string => {
  switch (face.unavailable_reason) {
    case "weight_not_expressed_by_text_v4":
      return "This weight is not supported by text editing yet.";
    case "system_font_not_redistributable":
      return "System fonts cannot be imported as licensed project resources.";
    default:
      return "No fixed static source is available for this face.";
  }
};
export const fontFaceLabel = (face: Pick<Face, "weight" | "style">): string =>
  `${face.weight === 400 ? t("Regular") : face.weight === 700 ? t("Bold") : face.weight} · ${t(face.style === "italic" ? "Italic" : "Normal")}`;
const faceKey = (face: Face) => `${face.weight}:${face.style}`;
const errorMessage = (error: unknown) =>
  error instanceof Error ? error.message : String(error);

export function FontBrowserImport(
  props: V4FontImportProps & {
    browserCatalog: FontBrowserCatalog;
    readSource: FontSourceReader;
  },
) {
  useLocale();
  const catalog = props.browserCatalog;
  const families = useMemo(
    () => mergeDerivedFontCatalog(catalog, props.derivedCatalog),
    [catalog, props.derivedCatalog],
  );
  const [query, setQuery] = useState("");
  const [familyName, setFamilyName] = useState(
    () =>
      families.find((family) => family.family === "Lato")?.family ??
      families.find((family) => family.faces.some((face) => face.catalog_id))
        ?.family ??
      "",
  );
  const [styleKey, setStyleKey] = useState("400:normal");
  const [text, setText] = useState(props.requiredText);
  const [source, setSource] = useState<FontSourceMetadata | null>(null);
  const [reading, setReading] = useState(false);
  const [readError, setReadError] = useState<string | null>(null);
  const [importError, setImportError] = useState<string | null>(null);
  const [retry, setRetry] = useState(0);
  const [pending, setPending] = useState(false);
  const inFlight = useRef(false),
    active = useRef(true),
    composing = useRef(false);
  const sourceReader = useRef(props.readSource);
  sourceReader.current = props.readSource;
  useEffect(() => {
    active.current = true;
    return () => {
      active.current = false;
    };
  }, []);
  const filtered = useMemo(() => {
    const needle = query.trim().toLocaleLowerCase();
    return families.filter((family) =>
      family.family.toLocaleLowerCase().includes(needle),
    );
  }, [families, query]);
  const family = families.find((item) => item.family === familyName);
  const face =
    family?.faces.find((item) => faceKey(item) === styleKey) ??
    family?.faces[0];
  const catalogId = face?.catalog_id ?? null;
  const admitted = source?.id === catalogId ? source : null;
  const locked = !!props.disabled || !!props.disabledReason || pending;
  const canImport =
    !locked &&
    (face?.derived
      ? props.derivedCatalog?.import_available
      : catalog.download_available) &&
    !!admitted &&
    !reading &&
    !!catalogId;

  useEffect(() => {
    const controller = new AbortController();
    let live = true;
    setSource(null);
    setReadError(null);
    setImportError(null);
    if (!catalogId || !face || !family) {
      setReading(false);
      return () => controller.abort();
    }
    setReading(true);
    const readingSource: Promise<FontSourceMetadata> =
      face.derived && props.readDerivedSource
        ? props
            .readDerivedSource(catalogId, controller.signal)
            .then(derivedSourceMetadata)
        : sourceReader.current(catalogId, controller.signal);
    void readingSource
      .then((result) => {
        if (!live) return;
        if (
          result.id !== catalogId ||
          result.family !== family.family ||
          result.weight !== face.weight ||
          result.style !== face.style ||
          !result.license_notice ||
          !result.content_hash ||
          result.source_commit !==
            (face.derived
              ? props.derivedCatalog?.source_commit
              : catalog.source_commit) ||
          (face.derived &&
            (result.derived?.face.derivation_id !==
              face.derived.derivation_id ||
              result.content_hash !== face.derived.content_hash ||
              result.size_bytes !== face.derived.size_bytes))
        ) {
          throw new Error(
            "The font source differs from the selected catalog face.",
          );
        }
        setSource(result);
      })
      .catch((error) => {
        if (live) setReadError(errorMessage(error));
      })
      .finally(() => {
        if (live) setReading(false);
      });
    return () => {
      live = false;
      controller.abort();
    };
  }, [
    catalogId,
    family?.family,
    face?.weight,
    face?.style,
    catalog.source_commit,
    props.derivedCatalog,
    face?.derived,
    props.scope.projectId,
    props.scope.revision,
    retry,
  ]);

  const submit = async () => {
    if (!canImport || inFlight.current || composing.current || !catalogId)
      return;
    inFlight.current = true;
    setPending(true);
    setImportError(null);
    try {
      if (face?.derived) await props.importFont(catalogId, text, "derived");
      else await props.importFont(catalogId, text);
      if (active.current) props.close();
    } catch (error) {
      if (active.current) setImportError(errorMessage(error));
    } finally {
      inFlight.current = false;
      if (active.current) setPending(false);
    }
  };
  const chooseFamily = (name: string) => {
    setFamilyName(name);
    setImportError(null);
    const next = families.find((item) => item.family === name);
    const preferred =
      next?.faces.find((item) => faceKey(item) === styleKey) ??
      next?.faces.find((item) => item.catalog_id) ??
      next?.faces[0];
    if (preferred) setStyleKey(faceKey(preferred));
  };
  return (
    <form
      className="font-browser"
      aria-label={t("Import font")}
      onSubmit={(event) => {
        event.preventDefault();
        void submit();
      }}
      onCompositionStart={() => {
        composing.current = true;
      }}
      onCompositionEnd={() => {
        composing.current = false;
      }}
      onKeyDown={(event) => {
        event.stopPropagation();
        if (
          event.key === "Escape" &&
          !event.nativeEvent.isComposing &&
          !composing.current
        ) {
          event.preventDefault();
          if (!inFlight.current) props.close();
        }
      }}
    >
      <h3>{t("Import font")}</h3>
      <p className="muted">
        {catalog.classic_family_count} {t("Google font families")} ·{" "}
        {catalog.pinned_face_count} {t("fixed static faces")}
      </p>
      {props.disabledReason && <p role="status">{t(props.disabledReason)}</p>}
      {!(face?.derived
        ? props.derivedCatalog?.import_available
        : catalog.download_available) && (
        <p role="status">
          {t(catalog.unavailable_reason ?? "Font import is unavailable.")}
        </p>
      )}
      <fieldset disabled={locked}>
        <Field label={t("Search fonts")}>
          <input
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
          />
        </Field>
        <p className="muted" aria-live="polite">
          {filtered.length} {t("matching families")}
        </p>
        <Field label={t("Font family")}>
          <select
            value={familyName}
            onChange={(event) => chooseFamily(event.target.value)}
          >
            {!filtered.some((item) => item.family === familyName) && family && (
              <option value={family.family}>{family.family}</option>
            )}
            {filtered.map((item) => (
              <option key={item.family} value={item.family}>
                {item.family}
                {item.category === "system"
                  ? ` · ${t("System font")}`
                  : item.faces.some((f) => f.catalog_id)
                    ? ""
                    : ` · ${t("Unavailable")}`}
              </option>
            ))}
          </select>
        </Field>
        {filtered.length === 0 && (
          <p role="status">{t("No matching font families.")}</p>
        )}
        <FontAtlasPreview family={familyName} />
        <Field label={t("Font style")}>
          <select
            value={face ? faceKey(face) : ""}
            onChange={(event) => {
              setStyleKey(event.target.value);
              setImportError(null);
            }}
          >
            {family?.faces.map((item) => (
              <option key={faceKey(item)} value={faceKey(item)}>
                {fontFaceLabel(item)}
                {item.catalog_id ? "" : ` · ${t("Unavailable")}`}
              </option>
            ))}
          </select>
        </Field>
        {face && !catalogId && (
          <p role="status">{t(fontFaceUnavailableMessage(face))}</p>
        )}
        {reading && (
          <p role="status">{t("Reading font source and license…")}</p>
        )}
        {face?.derived && !props.derivedCatalog?.import_available && (
          <p role="status">
            {t(
              props.derivedCatalog?.unavailable_reason ??
                "Derived font import is unavailable.",
            )}
          </p>
        )}
        {admitted?.derived && (
          <div className="font-browser-source">
            <p>{t("Verified static instance from a variable font")}</p>
            <p>
              {t("Derivation identity")}: {admitted.derived.face.derivation_id}
            </p>
            <p>
              {t("Variable source hash")}: {admitted.derived.source_sha256}
            </p>
            <p>
              {t("Static output hash")}: {admitted.derived.face.content_hash}
            </p>
            <p>
              {admitted.derived.axes
                .map((axis) => `${axis.tag}=${axis.coordinate}`)
                .join(" · ")}
            </p>
            <a
              href={admitted.derived.license_url}
              target="_blank"
              rel="noreferrer"
            >
              {t("Font license")}
            </a>
            <details>
              <summary>{t("Derivation proof")}</summary>
              <pre>{admitted.derived.proof_json}</pre>
            </details>
          </div>
        )}
        {admitted && (
          <div className="font-browser-source">
            <p className="muted">
              {admitted.family} · {fontFaceLabel(admitted)} ·{" "}
              {admitted.license_id}
            </p>
            <a href={admitted.source_url} target="_blank" rel="noreferrer">
              {t("Font source and license")}
            </a>
            <details>
              <summary>{t("License notice")}</summary>
              <pre>{admitted.license_notice}</pre>
            </details>
          </div>
        )}
        {readError && (
          <>
            <p role="alert">{t(readError)}</p>
            <Button onClick={() => setRetry((value) => value + 1)}>
              {t("Retry font source")}
            </Button>
          </>
        )}
        <Field label={t("Text to check")}>
          <textarea
            rows={3}
            value={text}
            onChange={(event) => {
              setText(event.target.value);
              setImportError(null);
            }}
          />
        </Field>
        <p className="muted">
          {t(
            "The selected font is checked against this text before it is added to your project.",
          )}
        </p>
      </fieldset>
      {importError && <p role="alert">{t(importError)}</p>}
      <div className="toolbar">
        <button className="primary" type="submit" disabled={!canImport}>
          {t(pending ? "Importing font…" : "Import font")}
        </button>
        <Button disabled={pending} onClick={props.close}>
          {t("Cancel")}
        </Button>
      </div>
    </form>
  );
}
