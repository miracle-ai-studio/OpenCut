import { useEffect, useRef, useState } from "react";
import { useEditingWorkspace } from "../../context";
import { t, useLocale } from "../../../shared/locale";
import { Button, Modal } from "../../../shared/ui";
import { useWorkbenchView } from "../workbench/view";
import { loadProjectFont } from "./canvas-font";
import { useFontApplication } from "./workspace";
import {
  displayFontFamily,
  requestedFontStyle,
  synthesisFont,
} from "./font-synthesis";
import type { TextFont } from "./types";

type FontLoad =
  | { phase: "loading" }
  | { phase: "ready"; family: string }
  | { phase: "failed"; error: string };

/** Only visible cards acquire scoped faces. Resource preview never selects or creates a timeline element. */
export function ProjectFontCard(props: { font: TextFont; active: boolean }) {
  const { api, scene } = useEditingWorkspace();
  const snapshot = scene?.historical ?? scene?.head;
  return (
    <ScopedFontCard
      key={JSON.stringify([
        api?.connectionIdentity,
        snapshot?.project.id,
        snapshot?.revision,
        scene?.sequenceId,
        props.font,
      ])}
      {...props}
    />
  );
}

function ScopedFontCard({ font, active }: { font: TextFont; active: boolean }) {
  useLocale();
  const { api, scene } = useEditingWorkspace();
  const { showProperties } = useWorkbenchView();
  const application = useFontApplication(font.id);
  const root = useRef<HTMLDivElement>(null);
  const [visible, setVisible] = useState(false);
  const [load, setLoad] = useState<FontLoad>({ phase: "loading" });
  const [attempt, setAttempt] = useState(0);
  const [preview, setPreview] = useState(false);
  const snapshot = scene?.historical ?? scene?.head;
  const projectId = snapshot?.project.id,
    revision = snapshot?.revision,
    sequenceId = scene?.sequenceId;
  useEffect(() => {
    if (!active) {
      setVisible(false);
      setPreview(false);
      return;
    }
    if (typeof IntersectionObserver !== "function") {
      setVisible(true);
      return;
    }
    const observer = new IntersectionObserver((entries) => {
      if (entries.some((entry) => entry.isIntersecting)) {
        setVisible(true);
        observer.disconnect();
      }
    });
    if (root.current) observer.observe(root.current);
    return () => observer.disconnect();
  }, [active]);
  useEffect(() => {
    setLoad({ phase: "loading" });
    if (!active || !visible || !api || !projectId || !revision || !sequenceId)
      return;
    if (typeof FontFace !== "function") {
      setLoad({
        phase: "failed",
        error: "Font preview is unavailable in this browser.",
      });
      return;
    }
    const abort = new AbortController();
    let release: (() => void) | undefined;
    void loadProjectFont(
      api,
      { projectId, revision, sequenceId },
      {
        fontId: font.id,
        contentHash: font.content.content_hash,
        size: String(font.content.size),
        weight: font.weight,
        style: font.style,
        faceIndex: font.face_index,
      },
      abort.signal,
    )
      .then((face) => {
        if (abort.signal.aborted) {
          face.release();
          return;
        }
        release = face.release;
        setLoad({ phase: "ready", family: face.family });
      })
      .catch((error) => {
        if (!abort.signal.aborted)
          setLoad({
            phase: "failed",
            error: error instanceof Error ? error.message : String(error),
          });
      });
    return () => {
      abort.abort();
      release?.();
    };
  }, [
    api,
    projectId,
    revision,
    sequenceId,
    font.id,
    font.content.content_hash,
    font.content.size,
    font.weight,
    font.style,
    font.face_index,
    active,
    visible,
    attempt,
  ]);
  const resolvedFont = application?.font ?? font;
  const style = requestedFontStyle(resolvedFont);
  const previewReady =
    load.phase === "ready" && (!synthesisFont(font) || !!application?.font);
  const styleName = [
    font.weight === 700
      ? t("Bold")
      : font.weight === 400
        ? t("Regular")
        : String(font.weight),
    style === "italic" ? t("Italic") : "",
  ]
    .filter(Boolean)
    .join(" · ");
  const label = `${displayFontFamily(resolvedFont)} · ${styleName}`;
  const disabledReason =
    load.phase === "failed"
      ? t("Font loading failed. Retry before applying.")
      : load.phase !== "ready"
        ? t("Loading font…")
        : t(
            application?.disabledReason ??
              (!application ? "Font application is unavailable." : ""),
          );
  const applyLabel = application?.targetName
    ? t("Apply {font} to {target}", {
        font: label,
        target: application.targetName,
      })
    : t("Apply font");
  const apply = () => {
    if (!disabledReason && application) {
      setPreview(false);
      showProperties();
      application.apply();
    }
  };
  const sampleStyle =
    load.phase === "ready"
      ? { fontFamily: load.family, fontWeight: font.weight, fontStyle: style }
      : undefined;
  return (
    <div ref={root} className="resource-font-card">
      <Button
        className="resource-text-card resource-font-preview"
        aria-label={t("Preview font {font}", { font: label })}
        title={label}
        disabled={!previewReady}
        onClick={() => setPreview(true)}
        onKeyDown={(event) => {
          if (event.key === "Enter" && event.shiftKey) {
            event.preventDefault();
            event.stopPropagation();
            if (!event.repeat) apply();
          }
        }}
      >
        <span
          className="resource-text-swatch"
          style={sampleStyle}
          aria-busy={load.phase === "loading"}
        >
          {previewReady ? (
            "Aa"
          ) : (
            <span className="resource-font-state">
              {t(
                load.phase === "failed"
                  ? "Font loading failed"
                  : "Loading font…",
              )}
            </span>
          )}
        </span>
        <strong>{label}</strong>
      </Button>
      {load.phase === "failed" ? (
        <div className="resource-font-feedback">
          <p role="alert">{t(load.error)}</p>
          <Button onClick={() => setAttempt((value) => value + 1)}>
            {t("Retry font loading")}
          </Button>
        </div>
      ) : (
        <Button
          className="resource-font-apply"
          disabled={!!disabledReason}
          aria-label={applyLabel}
          title={disabledReason || applyLabel}
          onClick={apply}
        >
          {t("Apply")}
        </Button>
      )}
      {active && preview && previewReady && (
        <Modal
          title={t("Preview font {font}", { font: label })}
          onClose={() => setPreview(false)}
          className="font-preview-dialog"
        >
          <div className="font-preview-sample" style={sampleStyle}>
            Aa 0123456789
          </div>
          <p>{label}</p>
          <p>
            {application?.targetName
              ? t("Target: {target}", { target: application.targetName })
              : t("Select one editable text or subtitle clip.")}
          </p>
          {disabledReason && <p role="status">{disabledReason}</p>}
          <Button
            disabled={!!disabledReason}
            aria-label={applyLabel}
            onClick={apply}
          >
            {t("Apply font")}
          </Button>
        </Modal>
      )}
    </div>
  );
}
