import { useEffect, useRef, useState } from "react";
import { Button, Field } from "../../../shared/ui";
import { t, useLocale } from "../../../shared/locale";
import { editableTextFonts, textDraft, textDraftValue } from "./draft";
import type { TextDraft } from "./draft";
import type { V4TextEditorProps } from "./types";

/** A form-only consumer. The host owns planning, original-intent recovery,
 * commit and native PNG refresh; there is no DOM text-layout preview here. */
export function V4TextEditor(props: V4TextEditorProps) {
  if (!props.available) return null;
  const { projectId, revision, sequenceId, elementId } = props.scope;
  return (
    <TextEditorForm
      key={JSON.stringify([
        projectId,
        revision,
        sequenceId,
        elementId,
        props.mode,
      ])}
      {...props}
    />
  );
}

function TextEditorForm(props: V4TextEditorProps) {
  useLocale();
  const [initial] = useState(() => structuredClone(props.initial));
  const [original] = useState(() => textDraft(initial));
  const [draft, setDraft] = useState(original);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inFlight = useRef(false),
    composing = useRef(false),
    active = useRef(true);
  useEffect(() => {
    active.current = true;
    return () => {
      active.current = false;
    };
  }, []);
  const fonts = editableTextFonts(props.fonts);
  const fontAvailable = fonts.some((font) => font.id === draft.fontId);
  const blocked = props.disabled || !!props.disabledReason || pending;
  const unchanged = JSON.stringify(draft) === JSON.stringify(original);
  const update = <K extends keyof TextDraft>(name: K, value: TextDraft[K]) => {
    setDraft((current) => ({ ...current, [name]: value }));
    setError(null);
  };
  const submit = async () => {
    if (
      blocked ||
      inFlight.current ||
      composing.current ||
      !fontAvailable ||
      (props.mode === "edit" && unchanged)
    )
      return;
    inFlight.current = true;
    setPending(true);
    setError(null);
    try {
      await props.save(textDraftValue(initial, draft, props.fonts));
      if (active.current) props.close();
    } catch (value) {
      if (active.current)
        setError(value instanceof Error ? value.message : String(value));
    } finally {
      inFlight.current = false;
      if (active.current) setPending(false);
    }
  };
  return (
    <form
      aria-label={t(props.mode === "create" ? "New text" : "Edit text")}
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
      <h3>{t(props.mode === "create" ? "New text" : "Edit text")}</h3>
      {props.disabledReason && <p role="status">{t(props.disabledReason)}</p>}
      {!fontAvailable && (
        <p role="status">
          {t("Choose an imported font face before saving text.")}
        </p>
      )}
      <fieldset disabled={blocked}>
        <Field label={t("Text name")}>
          <input
            value={draft.name}
            onChange={(event) => update("name", event.target.value)}
          />
        </Field>
        <Field label={t("Text content")}>
          <textarea
            rows={4}
            value={draft.content}
            onChange={(event) => update("content", event.target.value)}
          />
        </Field>
        <Field label={t("Font family")}>
          <select
            value={fontAvailable ? draft.fontId : ""}
            onChange={(event) => update("fontId", event.target.value)}
          >
            {!fontAvailable && (
              <option value="" disabled>
                {t("Choose an imported font face")}
              </option>
            )}
            {fonts.map((font) => (
              <option key={font.id} value={font.id}>
                {font.family} · {t(font.weight === 700 ? "Bold" : "Regular")}
                {font.style === "italic" ? ` · ${t("Italic")}` : ""}
              </option>
            ))}
          </select>
        </Field>
        <div className="field-pair">
          <Field
            label={t(
              initial.text.font_size.unit === "classic_height90"
                ? "Font size (Classic)"
                : "Font size (pixels)",
            )}
          >
            <input
              required
              inputMode="decimal"
              value={draft.size}
              onChange={(event) => update("size", event.target.value)}
            />
          </Field>
          <Field label={t("Text alignment")}>
            <select
              value={draft.align}
              onChange={(event) =>
                update("align", event.target.value as TextDraft["align"])
              }
            >
              <option value="left">{t("Left")}</option>
              <option value="center">{t("Center")}</option>
              <option value="right">{t("Right")}</option>
            </select>
          </Field>
        </div>
        <div className="field-pair">
          <Field label={t("Color")}>
            <input
              type="color"
              value={draft.color}
              onChange={(event) => update("color", event.target.value)}
            />
          </Field>
          <Field label={t("Opacity (%)")}>
            <input
              required
              inputMode="decimal"
              value={draft.opacity}
              onChange={(event) => update("opacity", event.target.value)}
            />
          </Field>
        </div>
        <Field label={t("Text decoration")}>
          <select
            value={draft.decoration}
            onChange={(event) =>
              update(
                "decoration",
                event.target.value as TextDraft["decoration"],
              )
            }
          >
            <option value="none">{t("None")}</option>
            <option value="underline">{t("Underline")}</option>
            <option value="line_through">{t("Strikethrough")}</option>
          </select>
        </Field>
        <div className="field-pair">
          <Field label={t("Letter spacing (pixels)")}>
            <input
              required
              inputMode="decimal"
              value={draft.letterSpacing}
              onChange={(event) => update("letterSpacing", event.target.value)}
            />
          </Field>
          <Field label={t("Line height")}>
            <input
              required
              inputMode="decimal"
              value={draft.lineHeight}
              onChange={(event) => update("lineHeight", event.target.value)}
            />
          </Field>
        </div>
        <div className="field-pair">
          <Field label={t("Start (seconds)")}>
            <input
              required
              inputMode="decimal"
              value={draft.start}
              onChange={(event) => update("start", event.target.value)}
            />
          </Field>
          <Field label={t("Duration (seconds)")}>
            <input
              required
              inputMode="decimal"
              value={draft.duration}
              onChange={(event) => update("duration", event.target.value)}
            />
          </Field>
        </div>
      </fieldset>
      {error && <p role="alert">{t(error)}</p>}
      <div className="toolbar">
        <button
          type="submit"
          className="primary"
          disabled={
            blocked || !fontAvailable || (props.mode === "edit" && unchanged)
          }
        >
          {t(pending ? "Saving text…" : "Save text")}
        </button>
        <Button disabled={pending} onClick={props.close}>
          {t("Cancel")}
        </Button>
      </div>
    </form>
  );
}
