import { t, useLocale } from "../shared/locale";
import { useRef, useState } from "react";
import { useWorkspace } from "../workspace/context";
import { activeSequence } from "../model/types";
import { Button } from "../shared/ui";
import { downloadBlob } from "../shared/download";
export function Subtitles() {
  useLocale();
  const { controller, api, snapshot, historical, sequenceId, busy } =
    useWorkspace();
  const view = historical ?? snapshot,
    sequence = activeSequence(view, sequenceId);
  const input = useRef<HTMLInputElement>(null),
    [pending, setPending] = useState(false);
  const importFile = async (file: File) => {
    if (!api || !snapshot || !sequence) return;
    setPending(true);
    try {
      if (file.size > 1024 * 1024)
        throw new Error("Subtitle files must be at most 1 MB.");
      const content = await file.text();
      if (controller.getSnapshot().snapshot?.revision !== snapshot.revision)
        throw new Error("The project changed. Import the subtitles again.");
      const result = await controller.write(() =>
        api.sdk.call("opencut_subtitle_import", {
          project_id: snapshot.project.id,
          expected_revision: snapshot.revision,
          sequence_id: sequence.id,
          idempotency_key: crypto.randomUUID(),
          format: file.name.toLowerCase().endsWith(".ass") ? "ass" : "srt",
          content,
          style: null,
        }),
      );
      controller.notify(
        `Imported ${result.cue_count} captions${result.warnings.length ? ` · ${result.warnings.join("; ")}` : ""}`,
      );
    } catch (error) {
      controller.report(error);
    } finally {
      setPending(false);
      if (input.current) input.current.value = "";
    }
  };
  const exportFile = async (format: "srt" | "ass") => {
    if (!api || !view || !sequence) return;
    try {
      const result = await api.sdk.call("opencut_subtitle_export", {
        project_id: view.project.id,
        revision: view.revision,
        sequence_id: sequence.id,
        format,
      });
      downloadBlob(
        new Blob([result.content], { type: "text/plain;charset=utf-8" }),
        `captions.${format}`,
      );
      if (result.losses.length) controller.notify(result.losses.join("; "));
    } catch (error) {
      controller.report(error);
    }
  };
  return (
    <fieldset>
      <legend>{t("Subtitle files")}</legend>
      <input
        className="hidden"
        ref={input}
        aria-label={t("Subtitle file")}
        type="file"
        accept=".srt,.ass"
        onChange={(event) => {
          const file = event.target.files?.[0];
          if (file) void importFile(file);
        }}
      />
      <Button
        disabled={!!historical || busy || pending}
        onClick={() => input.current?.click()}
      >
        {pending ? t("Importing…") : t("Import SRT / ASS")}
      </Button>
      <div className="toolbar">
        <Button
          disabled={!sequence?.caption_cues?.length}
          onClick={() => void exportFile("srt")}
        >
          {t("Save SRT")}
        </Button>
        <Button
          disabled={!sequence?.caption_cues?.length}
          onClick={() => void exportFile("ass")}
        >
          {t("Save ASS")}
        </Button>
      </div>
    </fieldset>
  );
}
