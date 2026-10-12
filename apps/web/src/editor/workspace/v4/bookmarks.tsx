import { t, useLocale } from "../../shared/locale";
import { useState } from "react";
import { Button, Field } from "../../shared/ui";
import { secondsForDisplay, timeFromDecimalSeconds } from "../../model/time";
import type { Time } from "../../model/types";
import type { V4Bookmark, V4BookmarkOperation } from "./types";

export function BookmarkEditor({
  bookmark,
  sequenceId,
  playhead,
  disabled,
  save,
  close,
}: {
  bookmark: V4Bookmark | null;
  sequenceId: string;
  playhead: Time;
  disabled: boolean;
  save: (op: V4BookmarkOperation) => Promise<unknown>;
  close: () => void;
}) {
  const [initial] = useState(() => ({
    time: bookmark?.time ?? playhead,
    duration: bookmark?.duration ?? null,
    color: bookmark?.color ?? null,
  }));
  const originalTime = secondsForDisplay(initial.time).toFixed(6);
  const originalDuration = initial.duration
    ? secondsForDisplay(initial.duration).toFixed(6)
    : "";
  useLocale();
  const [time, setTime] = useState(originalTime);
  const [duration, setDuration] = useState(originalDuration);
  const [note, setNote] = useState(bookmark?.note ?? "");
  const [color, setColor] = useState(bookmark?.color ?? "#facc15");
  const [pending, setPending] = useState(false),
    [error, setError] = useState<string | null>(null);
  const submit = async () => {
    setPending(true);
    setError(null);
    try {
      await save({
        kind: "put",
        bookmark: {
          ...bookmark,
          id: bookmark?.id ?? crypto.randomUUID(),
          sequence_id: sequenceId,
          time:
            time === originalTime ? initial.time : timeFromDecimalSeconds(time),
          duration:
            duration === originalDuration
              ? initial.duration
              : duration
                ? timeFromDecimalSeconds(duration)
                : null,
          note: note || null,
          color:
            color === (initial.color ?? "#facc15") && bookmark
              ? initial.color
              : color,
        },
      });
      close();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setPending(false);
    }
  };
  return (
    <form
      className="v4-bookmark-form"
      onSubmit={(e) => {
        e.preventDefault();
        void submit();
      }}
    >
      <h3>{t(bookmark ? "Edit bookmark" : "New bookmark")}</h3>
      <Field label={t("Bookmark time (seconds)")}>
        <input
          required
          inputMode="decimal"
          value={time}
          onChange={(e) => setTime(e.target.value)}
        />
      </Field>
      <Field label={t("Note duration (seconds, optional)")}>
        <input
          inputMode="decimal"
          value={duration}
          onChange={(e) => setDuration(e.target.value)}
        />
      </Field>
      <Field label={t("Canvas note")}>
        <textarea
          maxLength={4000}
          rows={3}
          value={note}
          onChange={(e) => setNote(e.target.value)}
        />
      </Field>
      <Field label={t("Bookmark color")}>
        <input
          type="color"
          value={color}
          onChange={(e) => setColor(e.target.value)}
        />
      </Field>
      {error && <p role="alert">{error}</p>}
      <div className="toolbar">
        <button
          type="submit"
          className="primary"
          disabled={disabled || pending}
        >
          {t("Save bookmark")}
        </button>
        <Button disabled={pending} onClick={close}>
          {t("Cancel")}
        </Button>
      </div>
    </form>
  );
}
