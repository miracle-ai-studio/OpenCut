import { useEffect, useRef, useState } from "react";
import { Button, Field } from "../../../shared/ui";
import { t, useLocale } from "../../../shared/locale";
import type { V4TextFont, V4TextScope } from "./types";
import type { DerivedFontCatalog, DerivedSourceReader } from "./font-derived";
import { FontBrowserImport } from "./font-browser";
import type { FontBrowserCatalog, FontSourceReader } from "./font-browser";

/** Display metadata supplied by the host's admitted catalog. Source and license
 * metadata are not user assertions and never form a download request here. */
export type FontImportChoice = {
  id: string;
  label: string;
  family: V4TextFont["family"];
  weight: V4TextFont["weight"];
  style: V4TextFont["style"];
  licenseName: string;
  sourceUrl: string;
};
export type V4FontImportProps = {
  scope: Pick<V4TextScope, "projectId" | "revision">;
  available: boolean;
  choices: readonly FontImportChoice[];
  browserCatalog?: FontBrowserCatalog;
  readSource?: FontSourceReader;
  derivedCatalog?: DerivedFontCatalog | null;
  readDerivedSource?: DerivedSourceReader;
  requiredText: string;
  disabled?: boolean;
  disabledReason?: string | null;
  importFont(
    choiceId: string,
    requiredText: string,
    sourceKind?: "derived",
  ): Promise<unknown>;
  close(): void;
};

export function V4FontImport(props: V4FontImportProps) {
  if (!props.available) return null;
  if (props.browserCatalog && props.readSource)
    return (
      <FontBrowserImport
        key={JSON.stringify([props.scope.projectId, props.scope.revision])}
        {...props}
        browserCatalog={props.browserCatalog}
        readSource={props.readSource}
      />
    );
  return (
    <FontImportForm
      key={JSON.stringify([props.scope.projectId, props.scope.revision])}
      {...props}
    />
  );
}

function FontImportForm(props: V4FontImportProps) {
  useLocale();
  const [selected, setSelected] = useState(props.choices[0]?.id ?? "");
  const [text, setText] = useState(props.requiredText);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inFlight = useRef(false),
    active = useRef(true),
    composing = useRef(false);
  useEffect(() => {
    active.current = true;
    return () => {
      active.current = false;
    };
  }, []);
  const choice = props.choices.find((item) => item.id === selected);
  const disabled = props.disabled || !!props.disabledReason || pending;
  const submit = async () => {
    if (disabled || inFlight.current || composing.current || !choice) return;
    inFlight.current = true;
    setPending(true);
    setError(null);
    try {
      await props.importFont(choice.id, text);
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
      {props.disabledReason && <p role="status">{t(props.disabledReason)}</p>}
      {!choice && (
        <p role="status">{t("No supported font faces are available.")}</p>
      )}
      <fieldset disabled={disabled}>
        <Field label={t("Font face")}>
          <select
            value={choice ? selected : ""}
            onChange={(event) => {
              setSelected(event.target.value);
              setError(null);
            }}
          >
            {!choice && (
              <option value="" disabled>
                {t("Choose a font face")}
              </option>
            )}
            {props.choices.map((item) => (
              <option key={item.id} value={item.id}>
                {item.label}
              </option>
            ))}
          </select>
        </Field>
        {choice && (
          <p className="muted">
            {choice.family} · {choice.weight} ·{" "}
            {t(choice.style === "italic" ? "Italic" : "Normal")} ·{" "}
            {choice.licenseName}
            {choice.sourceUrl.startsWith("https://") && (
              <>
                {" "}
                ·{" "}
                <a href={choice.sourceUrl} target="_blank" rel="noreferrer">
                  {t("Font source and license")}
                </a>
              </>
            )}
          </p>
        )}
        <Field label={t("Text to check")}>
          <textarea
            rows={3}
            value={text}
            onChange={(event) => {
              setText(event.target.value);
              setError(null);
            }}
          />
        </Field>
        <p className="muted">
          {t(
            "The selected font is checked against this text before it is added to your project.",
          )}
        </p>
      </fieldset>
      {error && <p role="alert">{t(error)}</p>}
      <div className="toolbar">
        <button
          className="primary"
          type="submit"
          disabled={disabled || !choice}
        >
          {t(pending ? "Importing font…" : "Import font")}
        </button>
        <Button disabled={pending} onClick={props.close}>
          {t("Cancel")}
        </Button>
      </div>
    </form>
  );
}
