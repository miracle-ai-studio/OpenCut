import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import { publicJobResult } from "../../../../lib/editor-sdk";
import type { ToolInput, ToolOutput } from "../../../../lib/editor-sdk";
import { useWorkspace } from "../../context";
import { Button, Field, Panel } from "../../../shared/ui";
import { downloadBlob } from "../../../shared/download";
import { t, useLocale } from "../../../shared/locale";
import { fontRecoveryBlocked } from "../text/font-recovery";
import type {
  CaptionContext,
  CaptionReceipt,
  V4CaptionRecovery,
} from "./recovery";
import {
  clearTranscriptionRequest,
  readTranscriptionRequest,
  writeTranscriptionRequest,
} from "./submission";
import type { TranscriptionRequestV4 } from "./submission";
import "./workspace.css";
export type CaptionWorkspaceOptions = {
  recovery: V4CaptionRecovery;
  mutateCaption(context: CaptionContext): Promise<CaptionReceipt>;
};
type Capability =
  ToolOutput<"opencut_analysis_sequence_transcribe_v4_capability">;
type Style = ToolInput<"opencut_subtitle_generate_v4">["style"];
const message = (e: unknown) => (e instanceof Error ? e.message : String(e));
export function V4CaptionWorkspace({
  recovery,
  mutateCaption,
}: CaptionWorkspaceOptions) {
  useLocale();
  const { api, v4, jobs, busy, controller } = useWorkspace();
  const restored = useSyncExternalStore(
    recovery.subscribe,
    recovery.getSnapshot,
  );
  const [capability, setCapability] = useState<Capability | null>(null),
    [diagnostic, setDiagnostic] = useState<string | null>(null);
  const [language, setLanguage] = useState("auto"),
    [words, setWords] = useState(3),
    [fontId, setFontId] = useState("");
  const [fontSize, setFontSize] = useState(24),
    [trackId, setTrackId] = useState(""),
    [jobId, setJobId] = useState("");
  const [format, setFormat] = useState<"srt" | "ass">("srt"),
    [content, setContent] = useState("");
  const [pending, setPending] = useState(false),
    [notice, setNotice] = useState<string | null>(null),
    [error, setError] = useState<string | null>(null);
  const [original, setOriginal] = useState<TranscriptionRequestV4 | null>(null);
  const epoch = useRef(0),
    writing = useRef(false);
  const snapshot = v4?.historical ?? v4?.head,
    scene = snapshot?.project.sequences.find((s) => s.id === v4?.sequenceId);
  const tracks =
      scene?.tracks.filter((track) => track.kind === "caption") ?? [],
    fonts = snapshot?.project.fonts ?? [];
  const selectedTrack = tracks.find((track) => track.id === trackId);
  const fixedJobs = jobs.filter(
    (job) =>
      job.kind === "analysis.sequence_transcribe.v4" &&
      job.target?.sequence_id === scene?.id,
  );
  const selectedJob = fixedJobs.find((job) => job.job_id === jobId);
  let ready = false;
  if (selectedJob?.state === "succeeded" && !selectedJob.result_unavailable) {
    try {
      const result = publicJobResult(
        selectedJob,
        "analysis.sequence_transcribe.v4",
      );
      ready =
        result.project_id === snapshot?.project.id &&
        result.revision === snapshot?.revision &&
        result.sequence_id === scene?.id;
    } catch {
      ready = false;
    }
  }
  const otherRecovery =
    !!v4?.recovery?.record ||
    !!v4?.recovery?.error ||
    ["reading", "staging", "applying"].includes(v4?.recovery?.phase ?? "") ||
    fontRecoveryBlocked(v4?.fontRecovery);
  const disabled =
    !api ||
    !snapshot ||
    !scene ||
    !!v4?.historical ||
    busy ||
    pending ||
    recovery.blocked() ||
    otherRecovery;
  const current = () => {
    const state = controller.getSnapshot();
    if (
      !api ||
      state.api !== api ||
      !state.v4 ||
      state.v4.historical ||
      !scene ||
      state.v4.head.project.id !== snapshot?.project.id ||
      state.v4.head.revision !== snapshot?.revision ||
      state.v4.sequenceId !== scene.id
    )
      throw new Error("The caption target changed.");
    return {
      project_id: state.v4.head.project.id,
      expected_revision: state.v4.head.revision,
      sequence_id: state.v4.sequenceId,
    };
  };
  useEffect(() => {
    epoch.current++;
    setNotice(null);
    setError(null);
    setJobId("");
    setContent("");
    setOriginal(null);
    setPending(false);
    if (!api?.connectionIdentity || !v4) return;
    try {
      setOriginal(
        readTranscriptionRequest(
          api.connectionIdentity,
          v4.head.project.id,
          v4.sequenceId,
        ),
      );
    } catch (e) {
      setError(message(e));
    }
  }, [api, api?.connectionIdentity, v4?.head.project.id, v4?.sequenceId]);
  useEffect(() => {
    if (!tracks.some((track) => track.id === trackId))
      setTrackId(tracks[0]?.id ?? "");
  }, [tracks, trackId]);
  useEffect(() => {
    if (!fonts.some((font) => font.id === fontId))
      setFontId(fonts[0]?.id ?? "");
  }, [fonts, fontId]);
  useEffect(() => {
    const cancel = new AbortController();
    setCapability(null);
    setDiagnostic(null);
    if (api)
      void api.sdk
        .call(
          "opencut_analysis_sequence_transcribe_v4_capability",
          {},
          { signal: cancel.signal },
        )
        .then((value) => {
          if (!cancel.signal.aborted) setCapability(value);
        })
        .catch((e) => {
          if (!cancel.signal.aborted) setDiagnostic(message(e));
        });
    return () => cancel.abort();
  }, [api]);
  async function run(action: (active: () => boolean) => Promise<void>) {
    if (writing.current) return;
    const serial = epoch.current;
    writing.current = true;
    setPending(true);
    setError(null);
    setNotice(null);
    try {
      await action(() => serial === epoch.current);
    } catch (e) {
      if (serial === epoch.current) setError(message(e));
    } finally {
      writing.current = false;
      if (serial === epoch.current) setPending(false);
    }
  }
  function style(): Style {
    if (
      !fonts.some((font) => font.id === fontId) ||
      !Number.isFinite(fontSize) ||
      fontSize <= 0 ||
      fontSize > 500
    )
      throw new Error(
        "Select a pinned project font and a size between 1 and 500 px.",
      );
    return {
      font: { kind: "locked", font_id: fontId },
      font_size: { unit: "canvas_pixels", value: fontSize },
      color: { encoding: "srgb", red: 1, green: 1, blue: 1, alpha: 1 },
      background_color: null,
      max_width_px: null,
      line_height_px: null,
      text_align: "center",
      layout: "pinned_caption_v4",
    };
  }
  const createTrack = () =>
    run(async (active) => {
      const scope = current();
      await controller.v4.write("Add caption track", [
        {
          type: "track_put",
          sequence_id: scope.sequence_id,
          track: {
            id: crypto.randomUUID(),
            name: "Captions",
            kind: "caption",
            enabled: true,
            muted: false,
            locked: false,
            opacity: 1,
            gain: { unit: "decibels", value: 0 },
            elements: [],
            channels: [],
          },
        },
      ]);
      if (!active()) return;
      setNotice("Caption track created. Transcribe this saved revision next.");
    });
  const transcribe = (retry: boolean) =>
    run(async (active) => {
      if (!api?.connectionIdentity)
        throw new Error("Authenticate the connection first.");
      const scope = current();
      if (!selectedTrack || selectedTrack.locked)
        throw new Error(
          "Choose an unlocked caption track before transcribing.",
        );
      if (!capability?.available)
        throw new Error(
          capability?.unavailable_reason ??
            "Local transcription is unavailable.",
        );
      const request =
        retry && original
          ? original
          : {
              project_id: scope.project_id,
              revision: scope.expected_revision,
              sequence_id: scope.sequence_id,
              language,
              idempotency_key: crypto.randomUUID(),
            };
      writeTranscriptionRequest(api.connectionIdentity, request);
      setOriginal(request);
      const accepted = await api.sdk.call(
        "opencut_analysis_sequence_transcribe_v4",
        request,
      );
      if (
        accepted.kind !== "analysis.sequence_transcribe.v4" ||
        accepted.target?.project_id !== request.project_id ||
        accepted.target?.revision !== request.revision ||
        accepted.target?.sequence_id !== request.sequence_id
      )
        throw new Error(
          "Accepted transcription differs from the original scene.",
        );
      clearTranscriptionRequest(api.connectionIdentity, request);
      if (!active()) return;
      setOriginal(null);
      setJobId(accepted.job_id);
      await controller.sync();
      if (!active()) return;
      setNotice(
        "Local transcription queued. The task list retains progress and cancellation.",
      );
    });
  const generate = () =>
    run(async (active) => {
      const scope = current();
      if (!ready || !selectedJob || !selectedTrack || selectedTrack.locked)
        throw new Error(
          "Select a successful transcript from this exact saved revision and an unlocked caption track.",
        );
      const receipt = await mutateCaption({
        sequence_id: scope.sequence_id,
        intent: {
          kind: "subtitle_generate_v4",
          request: {
            ...scope,
            track_id: trackId,
            analysis_job_id: selectedJob.job_id,
            idempotency_key: crypto.randomUUID(),
            words_per_caption: words,
            style: style(),
          },
        },
      });
      if (!active()) return;
      setNotice(
        receipt.cue_count + " captions saved. " + receipt.warnings.join(" "),
      );
    });
  const importFile = () =>
    run(async (active) => {
      const scope = current();
      if (!selectedTrack || selectedTrack.locked)
        throw new Error("Choose an unlocked caption track.");
      if (
        new TextEncoder().encode(content).length > 2 * 1024 * 1024 ||
        !content.trim()
      )
        throw new Error("Provide an SRT or ASS file up to 2 MiB.");
      const receipt = await mutateCaption({
        sequence_id: scope.sequence_id,
        intent: {
          kind: "subtitle_import_v4",
          request: {
            ...scope,
            track_id: trackId,
            idempotency_key: crypto.randomUUID(),
            format,
            content,
            style: style(),
          },
        },
      });
      if (!active()) return;
      setContent("");
      if (!active()) return;
      setNotice(
        receipt.cue_count + " captions imported. " + receipt.warnings.join(" "),
      );
    });
  const exportFile = () =>
    run(async (active) => {
      if (!api || !snapshot || !scene) return;
      const result = await api.sdk.call("opencut_subtitle_export_v4", {
        project_id: snapshot.project.id,
        revision: snapshot.revision,
        sequence_id: scene.id,
        track_id: trackId || null,
        format,
      });
      if (
        result.project_id !== snapshot.project.id ||
        result.revision !== snapshot.revision ||
        result.sequence_id !== scene.id
      )
        throw new Error(
          "Subtitle export differs from the selected fixed revision.",
        );
      if (!active()) return;
      downloadBlob(
        new Blob([result.content], {
          type:
            format === "srt"
              ? "application/x-subrip;charset=utf-8"
              : "text/x-ssa;charset=utf-8",
        }),
        "captions." + format,
      );
      setNotice(
        result.cue_count + " captions exported. " + result.losses.join(" "),
      );
    });
  if (!snapshot || !scene) return null;
  return (
    <Panel title={t("Captions")} className="v4-caption-workspace">
      <div className="caption-diagnostics" role="status">
        <strong>
          {capability?.available
            ? t("Local transcription ready")
            : t("Local transcription unavailable")}
        </strong>
        <span>
          {diagnostic ??
            capability?.unavailable_reason ??
            capability?.timing ??
            t("Checking local transcription…")}
        </span>
        <small>
          {t(
            "Captions wrap to 80% of the canvas and sit 5% above its bottom edge. Word timing is estimated within speech segments.",
          )}
        </small>
      </div>
      {restored.record && (
        <div className="caption-recovery" role="status">
          <strong>{t("Resolve the original caption edit")}</strong>
          <p>
            {restored.receipt
              ? t("The original receipt is saved. Acknowledge it to continue.")
              : t(
                  "Retry the saved original request without changing its revision or key.",
                )}
          </p>
          <div className="toolbar">
            <Button
              disabled={busy || pending || !!v4?.historical}
              onClick={() =>
                void run(async () => {
                  await recovery.retry();
                })
              }
            >
              {t("Retry original captions")}
            </Button>
            <Button
              disabled={busy || pending}
              onClick={() =>
                void run(async () => {
                  await recovery.release();
                })
              }
            >
              {t("Acknowledge original result")}
            </Button>
          </div>
          {restored.error && <p role="alert">{restored.error}</p>}
        </div>
      )}
      <div className="caption-fields">
        <Field label={t("Caption track")}>
          <select
            value={trackId}
            disabled={disabled}
            onChange={(e) => setTrackId(e.target.value)}
          >
            <option value="">{t("Choose a caption track")}</option>
            {tracks.map((track) => (
              <option key={track.id} value={track.id}>
                {track.name}
                {track.locked ? " · " + t("locked") : ""}
              </option>
            ))}
          </select>
        </Field>
        <Button disabled={disabled} onClick={() => void createTrack()}>
          {t("Add caption track")}
        </Button>
        <Field label={t("Pinned font")}>
          <select
            value={fontId}
            disabled={disabled}
            onChange={(e) => setFontId(e.target.value)}
          >
            <option value="">{t("Import a font first")}</option>
            {fonts.map((font) => (
              <option key={font.id} value={font.id}>
                {font.family} · {font.weight}
              </option>
            ))}
          </select>
        </Field>
        <Field label={t("Size (px)")}>
          <input
            type="number"
            min="1"
            max="500"
            value={fontSize}
            disabled={disabled}
            onChange={(e) => setFontSize(Number(e.target.value))}
          />
        </Field>
      </div>
      <div className="caption-columns">
        <section aria-label={t("Automatic captions")}>
          <h3>{t("Automatic captions")}</h3>
          <div className="caption-fields">
            <Field label={t("Speech language")}>
              <select
                value={language}
                disabled={disabled || !!original}
                onChange={(e) => setLanguage(e.target.value)}
              >
                {(capability?.supported_languages ?? ["auto", "en", "zh"]).map(
                  (code) => (
                    <option key={code} value={code}>
                      {code === "auto"
                        ? t("Detect automatically")
                        : code === "en"
                          ? t("English")
                          : code === "zh"
                            ? t("Chinese")
                            : code}
                    </option>
                  ),
                )}
              </select>
            </Field>
            <Field label={t("Words per caption")}>
              <input
                type="number"
                min="1"
                max="20"
                value={words}
                disabled={disabled}
                onChange={(e) => setWords(Number(e.target.value))}
              />
            </Field>
          </div>
          <div className="toolbar">
            <Button
              disabled={
                disabled ||
                !capability?.available ||
                !selectedTrack ||
                selectedTrack.locked ||
                !!original
              }
              onClick={() => void transcribe(false)}
            >
              {t("Transcribe saved scene")}
            </Button>
            {original && (
              <Button
                disabled={disabled || !capability?.available}
                onClick={() => void transcribe(true)}
              >
                {t("Retry original transcription")}
              </Button>
            )}
          </div>
          <Field label={t("Scene transcription")}>
            <select
              value={jobId}
              onChange={(e) => setJobId(e.target.value)}
              disabled={pending}
            >
              <option value="">{t("Choose a saved task")}</option>
              {fixedJobs.map((job) => (
                <option key={job.job_id} value={job.job_id}>
                  {job.state} · {job.target?.revision.slice(0, 12)} ·{" "}
                  {job.job_id.slice(-8)}
                </option>
              ))}
            </select>
          </Field>
          {selectedJob && (
            <div className="caption-task" role="status">
              <span>
                {t(selectedJob.state)}{" "}
                {selectedJob.progress?.fraction != null
                  ? Math.round(selectedJob.progress.fraction * 100) + "%"
                  : ""}
              </span>
              {selectedJob.failure && (
                <p role="alert">{selectedJob.failure.message}</p>
              )}
              {selectedJob.state === "succeeded" && !ready && (
                <small>
                  {t(
                    "This transcript belongs to another saved revision. Transcribe the current scene before generating captions.",
                  )}
                </small>
              )}
              {["queued", "running"].includes(selectedJob.state) && (
                <Button
                  disabled={pending}
                  onClick={() =>
                    void run(async (active) => {
                      await api?.sdk.cancelPublicJob(selectedJob.job_id);
                      if (!active()) return;
                      await controller.sync();
                    })
                  }
                >
                  {t("Cancel transcription")}
                </Button>
              )}
            </div>
          )}
          <div className="toolbar">
            <Button
              disabled={
                disabled ||
                !ready ||
                !fontId ||
                !selectedTrack ||
                selectedTrack.locked ||
                !Number.isInteger(words) ||
                words < 1 ||
                words > 20
              }
              onClick={() => void generate()}
            >
              {t("Generate captions")}
            </Button>
            <Button disabled={pending} onClick={() => void controller.sync()}>
              {t("Refresh tasks")}
            </Button>
          </div>
        </section>
        <section aria-label={t("Subtitle files")}>
          <h3>{t("Subtitle files")}</h3>
          <Field label={t("Format")}>
            <select
              value={format}
              disabled={pending}
              onChange={(e) => setFormat(e.target.value as "srt" | "ass")}
            >
              <option value="srt">SRT</option>
              <option value="ass">ASS</option>
            </select>
          </Field>
          <Field label={t("Open subtitle file")}>
            <input
              type="file"
              accept=".srt,.ass,text/plain"
              disabled={disabled}
              onChange={(e) => {
                const file = e.target.files?.[0];
                if (!file) return;
                if (file.size > 2 * 1024 * 1024) {
                  setError("Subtitle files must be at most 2 MiB.");
                  return;
                }
                const serial = epoch.current;
                void file
                  .text()
                  .then((text) => {
                    if (serial === epoch.current) {
                      setContent(text);
                      setFormat(
                        file.name.toLowerCase().endsWith(".ass")
                          ? "ass"
                          : "srt",
                      );
                    }
                  })
                  .catch((err) => setError(message(err)));
              }}
            />
          </Field>
          <Field label={t("Subtitle content")}>
            <textarea
              rows={5}
              value={content}
              disabled={disabled}
              onChange={(e) => setContent(e.target.value)}
              placeholder={t("Paste SRT or ASS text")}
            />
          </Field>
          <div className="toolbar">
            <Button
              disabled={
                disabled ||
                !content.trim() ||
                !fontId ||
                !selectedTrack ||
                selectedTrack.locked
              }
              onClick={() => void importFile()}
            >
              {t("Import subtitles")}
            </Button>
            <Button
              disabled={!api || pending || busy || !scene.captions.length}
              onClick={() => void exportFile()}
            >
              {t("Export subtitles")}
            </Button>
          </div>
          <small>
            {t(
              "SRT stores text and timing. ASS import uses your selected pinned style; export reports font and layout limitations.",
            )}
          </small>
        </section>
      </div>
      {restored.error && !restored.record && (
        <div role="alert">
          <p>{restored.error}</p>
          <Button
            disabled={busy || pending || !api}
            onClick={() =>
              void run(async () => {
                if (api) await recovery.restore(api, snapshot.project.id);
              })
            }
          >
            {t("Inspect original caption recovery")}
          </Button>
        </div>
      )}
      {!fonts.length && (
        <p>{t("Import a project font in Text before creating captions.")}</p>
      )}
      {error && (
        <p className="notice danger" role="alert">
          {error}
        </p>
      )}
      {notice && (
        <p className="notice" role="status">
          {notice}
        </p>
      )}
    </Panel>
  );
}
