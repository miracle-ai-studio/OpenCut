import { Select } from "../../../shared/controls";
import { useEffect, useMemo, useRef, useState } from "react";
import type { ToolOutput } from "../../../../lib/editor-sdk";
import { Button, Field } from "../../../shared/ui";
import { t, useLocale } from "../../../shared/locale";
import type { FontImportProps } from "./font-import";
import "./font-browser.css";
import { mergeFontCatalog } from "./font-derived";
import type { FontSourceMetadata } from "./font-derived";
import { unifiedSourceMetadata } from "./font-catalog";
import { FontAtlasPreview } from "./font-atlas-preview";
import {
  fontSynthesisText,
  fontRenderProfileLabel,
  synthesisBaseRouteLabel,
} from "./font-synthesis";

export type FontBrowserCatalog = ToolOutput<"opencut_font_catalog">;
export type FontBrowserSource = ToolOutput<"opencut_font_source">;
type Family = FontBrowserCatalog["families"][number];
type Face = Family["faces"][number];

export const fontFaceUnavailableMessage = (face: Face): string => {
  switch (face.unavailable_reason) {
    case "unsupported_weight":
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

export function FontBrowserImport(props: FontImportProps) {
  const { language } = useLocale();
  const synthesisText = (message: string) =>
    fontSynthesisText(message, language);
  const catalog = props.catalog;
  const families = useMemo(() => mergeFontCatalog(catalog), [catalog]);
  const [query, setQuery] = useState("");
  const [familyName, setFamilyName] = useState(
    () =>
      families.find((family) => family.family === "Lato")?.family ??
      families.find((family) => family.faces.some((face) => face.catalog_id))
        ?.family ??
      "",
  );
  const [styleKey, setStyleKey] = useState("400:normal");
  const [sourceMode, setSourceMode] = useState<"classic" | "real_italic">(
    "classic",
  );
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
  const slot =
    family?.faces.find((item) => faceKey(item) === styleKey) ??
    family?.faces[0];
  const face =
    sourceMode === "real_italic" && slot?.realItalic ? slot.realItalic : slot;
  const catalogId = face?.catalog_id ?? null;
  const admitted = source?.id === catalogId ? source : null;
  const locked = !!props.disabled || !!props.disabledReason || pending;
  const importAvailable = face?.unified?.import_available ?? false;
  const canImport =
    !locked && importAvailable && !!admitted && !reading && !!catalogId;

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
    const readingSource: Promise<FontSourceMetadata> = face.unified
      ? sourceReader
          .current({ catalog_id: catalogId }, controller.signal)
          .then((result) => unifiedSourceMetadata(result, face.unified!))
      : Promise.reject(
          new Error("No approved source is available for this font face."),
        );
    void readingSource
      .then((result) => {
        if (!live) return;
        if (
          result.id !== catalogId ||
          result.family !== (face.unified?.family ?? family.family) ||
          result.weight !== face.weight ||
          (result.requested_style ?? result.style) !== face.style ||
          !result.license_notice ||
          !result.content_hash
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
    face?.unified,
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
      await props.importFont(catalogId, text);
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
    setSourceMode("classic");
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
        {catalog.fonts.length} {synthesisText("Pinned font sources")}
      </p>
      {props.disabledReason && <p role="status">{t(props.disabledReason)}</p>}
      {!importAvailable && (
        <p role="status">
          {t(
            face?.unified?.unavailable_reason ??
              catalog.unavailable_reason ??
              "Font import is unavailable.",
          )}
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
          <Select
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
          </Select>
        </Field>
        {filtered.length === 0 && (
          <p role="status">{t("No matching font families.")}</p>
        )}
        <FontAtlasPreview family={familyName} />
        <Field label={t("Font style")}>
          <Select
            value={face ? faceKey(face) : ""}
            onChange={(event) => {
              setStyleKey(event.target.value);
              setSourceMode("classic");
              setImportError(null);
            }}
          >
            {family?.faces.map((item) => (
              <option key={faceKey(item)} value={faceKey(item)}>
                {fontFaceLabel(item)}
                {item.catalog_id ? "" : ` · ${t("Unavailable")}`}
              </option>
            ))}
          </Select>
        </Field>
        {slot?.realItalic && (
          <Field label={synthesisText("Italic source")}>
            <Select
              value={sourceMode}
              onChange={(event) => {
                setSourceMode(event.target.value as "classic" | "real_italic");
                setImportError(null);
              }}
            >
              <option value="classic">
                {synthesisText("Classic Canvas italic synthesis")}
              </option>
              <option value="real_italic">
                {synthesisText("Real italic font")}
              </option>
            </Select>
          </Field>
        )}
        {face && !catalogId && (
          <p role="status">{t(fontFaceUnavailableMessage(face))}</p>
        )}
        {reading && (
          <p role="status">{t("Reading font source and license…")}</p>
        )}
        {face?.unified?.alias_reason && (
          <p className="muted">{face.unified.alias_reason}</p>
        )}
        {admitted && face?.unified && admitted.family !== family?.family && (
          <p className="muted">
            {t("Imported font family")}: {admitted.family}
          </p>
        )}
        {admitted?.distribution && (
          <p className="muted">
            {t("Distribution release")}:{" "}
            {admitted.distribution.provenance.release}
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
        {admitted?.synthesis && (
          <div className="font-browser-source">
            <p>{synthesisText("Classic Canvas italic synthesis")}</p>
            <p>
              {synthesisText("Actual font")}: {admitted.family} ·{" "}
              {admitted.weight} · {admitted.style}
            </p>
            <p>
              {synthesisText("Requested style")}:{" "}
              {admitted.synthesis.binding.requested.style}
            </p>
            <p>
              {synthesisText(
                "Synthesis preserves the base font bytes and full license. The base source may have been derived.",
              )}
            </p>
            {admitted.synthesis_base && (
              <>
                <p>
                  {synthesisText("Classic family")}:{" "}
                  {admitted.synthesis_base.display_family}
                </p>
                <p>
                  {synthesisText("Base acquisition")}:{" "}
                  {synthesisText(
                    synthesisBaseRouteLabel(admitted.synthesis_base.route),
                  )}
                </p>
                <p>
                  {synthesisText("Base catalog identity")}:{" "}
                  {admitted.synthesis_base.catalog_id}
                </p>
                {(admitted.synthesis_base.route === "derived_v1" ||
                  admitted.synthesis_base.route === "derived_v2") && (
                  <>
                    <p>
                      {synthesisText("Base derivation identity")}:{" "}
                      {admitted.synthesis_base.source.face.derivation_id}
                    </p>
                    <p>
                      {synthesisText("Upstream source hash")}:{" "}
                      {admitted.synthesis_base.source.source_sha256}
                    </p>
                    <p>
                      {synthesisText("Synthesis input hash")}:{" "}
                      {admitted.content_hash}
                    </p>
                    <p>
                      {admitted.synthesis_base.source.axes
                        .map((axis) => `${axis.tag}=${axis.coordinate}`)
                        .join(" · ")}
                    </p>
                  </>
                )}
                <details>
                  <summary>{synthesisText("Base acquisition proof")}</summary>
                  <pre>{JSON.stringify(admitted.synthesis_base, null, 2)}</pre>
                </details>
              </>
            )}
            <details>
              <summary>{synthesisText("Font synthesis proof")}</summary>
              <pre>{admitted.synthesis.source.proof_json}</pre>
            </details>
          </div>
        )}
        {admitted && (
          <div className="font-browser-source">
            {admitted.render_profile && (
              <p>
                {synthesisText("Native rendering profile")}:{" "}
                {synthesisText(fontRenderProfileLabel(admitted.render_profile))}
              </p>
            )}
            <p className="muted">
              {admitted.family} · {fontFaceLabel(admitted)} ·{" "}
              {admitted.license_id}
            </p>
            <a href={admitted.source_url} target="_blank" rel="noreferrer">
              {admitted.synthesis_base
                ? synthesisText("Upstream source")
                : t("Font source and license")}
            </a>
            {admitted.license_url && (
              <a href={admitted.license_url} target="_blank" rel="noreferrer">
                {t("Font license")}
              </a>
            )}
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
