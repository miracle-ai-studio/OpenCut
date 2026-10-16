import { useEffect, useRef, useState } from "react";
import type { EditorApi } from "../../api";
import { loadCanvasFont } from "./canvas-font";
import { canvasTextGeometry } from "./canvas-geometry";
import type { CanvasTextPlan } from "./canvas-types";
import { t, useLocale } from "../../../shared/locale";
import "./canvas-input.css";

export function CanvasTextInput({
  plan,
  api,
  viewportWidth,
  commit,
  cancel,
}: {
  plan: CanvasTextPlan;
  api: EditorApi;
  viewportWidth: number;
  commit(content: string): Promise<void>;
  cancel(): void;
}) {
  useLocale();
  const [content, setContent] = useState(plan.layer.text.content);
  const [family, setFamily] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  const input = useRef<HTMLTextAreaElement>(null);
  const composing = useRef(false),
    deferred = useRef(false),
    ended = useRef(false),
    saving = useRef(false);
  const latest = useRef(content);
  latest.current = content;
  useEffect(() => {
    const abort = new AbortController();
    let font: Awaited<ReturnType<typeof loadCanvasFont>> | undefined;
    void loadCanvasFont(
      api,
      plan.scope,
      plan.layer.font,
      abort.signal,
      plan.actualFont,
    )
      .then((loaded) => {
        font = loaded;
        if (abort.signal.aborted) loaded.release();
        else setFamily(loaded.family);
      })
      .catch((value) => {
        if (!abort.signal.aborted)
          setError(value instanceof Error ? value.message : String(value));
      });
    return () => {
      abort.abort();
      font?.release();
    };
  }, [api, plan]);
  useEffect(() => {
    if (family) {
      input.current?.focus();
      input.current?.select();
    }
  }, [family]);
  const finish = async () => {
    if (ended.current || saving.current) return;
    if (composing.current) {
      deferred.current = true;
      return;
    }
    if (!family) return;
    saving.current = true;
    setPending(true);
    setError(null);
    try {
      await commit(latest.current);
      ended.current = true;
    } catch (value) {
      setError(value instanceof Error ? value.message : String(value));
      input.current?.focus();
    } finally {
      saving.current = false;
      setPending(false);
    }
  };
  const geometry = canvasTextGeometry(plan, viewportWidth),
    text = plan.layer.text;
  const draftHeight = Math.max(
    text.lineHeightPx,
    content.split("\n").length * text.lineHeightPx,
  );
  return (
    <div
      className="v4-canvas-text-session"
      onKeyDown={(event) => {
        event.stopPropagation();
        if (event.key === "Escape") {
          event.preventDefault();
          ended.current = true;
          deferred.current = false;
          cancel();
        } else if (
          (event.metaKey || event.ctrlKey) &&
          event.key === "Enter" &&
          !event.nativeEvent.isComposing &&
          !composing.current
        ) {
          event.preventDefault();
          void finish();
        }
      }}
    >
      {family && (
        <div
          className="v4-canvas-text-position"
          style={{ transform: geometry.transform }}
        >
          <textarea
            ref={input}
            aria-label={t("Edit text on canvas")}
            spellCheck={false}
            wrap="off"
            value={content}
            readOnly={pending}
            style={{
              left: geometry.left,
              top: -draftHeight / 2,
              width: geometry.width + 2,
              height: draftHeight,
              fontFamily: `"${family}"`,
              fontWeight: plan.layer.font.weight,
              fontStyle: plan.layer.font.style,
              fontSize: text.fontSizePx,
              lineHeight: `${text.lineHeightPx}px`,
              letterSpacing: text.letterSpacingPx,
              textAlign: text.align as "left" | "center" | "right",
              color: `rgba(${text.color
                .slice(0, 3)
                .map((v) => v * 255)
                .join(",")},${text.color[3]})`,
            }}
            onChange={(event) => {
              latest.current = event.target.value;
              setContent(event.target.value);
            }}
            onCompositionStart={() => {
              composing.current = true;
            }}
            onCompositionEnd={(event) => {
              composing.current = false;
              latest.current = event.currentTarget.value;
              setContent(event.currentTarget.value);
              if (deferred.current) {
                deferred.current = false;
                queueMicrotask(() => void finish());
              }
            }}
            onBlur={(event) => {
              if (
                !event.relatedTarget ||
                !event.currentTarget.parentElement?.parentElement?.contains(
                  event.relatedTarget,
                )
              )
                void finish();
            }}
          />
        </div>
      )}
      <div
        className="v4-canvas-text-tools"
        role="group"
        aria-label={t("Edit text on canvas")}
      >
        {!family && !error && (
          <span role="status">{t("Loading locked font…")}</span>
        )}
        {error && <span role="alert">{t(error)}</span>}
        <button
          type="button"
          disabled={!family || pending}
          onClick={() => void finish()}
        >
          {t("Confirm text")}
        </button>
        <button
          type="button"
          disabled={pending}
          onClick={() => {
            ended.current = true;
            cancel();
          }}
        >
          {t("Cancel text edit")}
        </button>
      </div>
    </div>
  );
}
