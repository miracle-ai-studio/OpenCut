import { t, useLocale } from "../shared/locale";
import { useEffect, useRef, useState } from "react";
import type { ToolOutput } from "../../lib/editor-sdk";
import { useWorkspace } from "../workspace/context";
import { activeSequence } from "../model/types";
import { Button, Field, Modal } from "../shared/ui";
export function ExportDialog({ onClose }: { onClose: () => void }) {
  useLocale();
  const { controller, api, snapshot, historical, sequenceId } = useWorkspace();
  const view = historical ?? snapshot,
    sequence = activeSequence(view, sequenceId);
  const [container, setContainer] = useState<"mp4" | "matroska">("mp4"),
    [codec, setCodec] = useState<"h264" | "hevc" | "vp9">("h264");
  const [report, setReport] = useState<{
    identity: string;
    value: ToolOutput<"opencut_render_preflight">;
  } | null>(null);
  const [checking, setChecking] = useState(false),
    [submitting, setSubmitting] = useState(false),
    [error, setError] = useState<string | null>(null);
  const identity = JSON.stringify([
    api?.sdk.baseUrl,
    view?.project.id,
    view?.revision,
    sequence?.id,
    container,
    codec,
  ]);
  const current = useRef(identity);
  current.current = identity;
  const attempts = useRef(new Map<string, string>()),
    checkGeneration = useRef(0);
  useEffect(
    () => () => {
      checkGeneration.current++;
    },
    [],
  );
  const inspect = async () => {
    if (!api || !view || !sequence) return;
    const generation = ++checkGeneration.current;
    setChecking(true);
    setError(null);
    try {
      const value = await api.sdk.call(
        view.format === "visual"
          ? "opencut_render_v3_preflight"
          : "opencut_render_preflight",
        {
          project_id: view.project.id,
          revision: view.revision,
          sequence_id: sequence.id,
          container,
          video_codec: codec,
        },
      );
      if (
        generation === checkGeneration.current &&
        current.current === identity
      )
        setReport({ identity, value });
    } catch (error) {
      if (generation === checkGeneration.current) setError(String(error));
    } finally {
      if (generation === checkGeneration.current) setChecking(false);
    }
  };
  const ready = report?.identity === identity && report.value.ready;
  const submit = async () => {
    if (!api || !view || !sequence || !ready) return;
    setSubmitting(true);
    setError(null);
    let key = attempts.current.get(identity);
    if (!key) {
      key = crypto.randomUUID();
      attempts.current.set(identity, key);
    }
    try {
      const input = {
        project_id: view.project.id,
        revision: view.revision,
        sequence_id: sequence.id,
        container,
        video_codec: codec,
      };
      await (view.format === "visual"
        ? api.sdk.call("opencut_job_v3_render", {
            ...input,
            idempotency_key: key,
          })
        : api.sdk.submitJob({
            kind: "render.project",
            input,
            idempotency_key: key,
          }));
      controller.notify("Export started");
      void controller.sync();
      onClose();
    } catch (error) {
      setError(String(error));
    } finally {
      setSubmitting(false);
    }
  };
  return (
    <Modal title={t("Export project")} onClose={onClose}>
      <p className="muted">
        {view?.project.name} · {sequence?.width} × {sequence?.height}
      </p>
      <div className="field-pair">
        <Field label={t("Format")}>
          <select
            value={container}
            disabled={checking || submitting}
            onChange={(event) => {
              const value = event.target.value as typeof container;
              setContainer(value);
              setCodec(value === "matroska" ? "vp9" : "h264");
            }}
          >
            <option value="mp4">{t("MP4")}</option>
            <option value="matroska">{t("Matroska")}</option>
          </select>
        </Field>
        <Field label={t("Codec")}>
          <select
            value={codec}
            disabled={checking || submitting}
            onChange={(event) => setCodec(event.target.value as typeof codec)}
          >
            {(container === "matroska" ? ["vp9"] : ["h264", "hevc"]).map(
              (codec) => (
                <option key={codec}>{codec}</option>
              ),
            )}
          </select>
        </Field>
      </div>
      {report?.identity === identity && (
        <div className={ready ? "success-note" : "error"} role="status">
          {ready
            ? t("Ready to export")
            : report.value.issues.map((issue) => (
                <p key={`${issue.code}-${issue.clip_id}-${issue.asset_id}`}>
                  {issue.message}
                </p>
              ))}
        </div>
      )}
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      <p className="muted">
        {t(
          "The export continues when you close this page. Find progress and downloads in Tasks.",
        )}
      </p>
      <div className="modal-footer">
        <Button onClick={onClose}>{t("Close")}</Button>
        <Button
          disabled={checking || submitting}
          onClick={() => void inspect()}
        >
          {checking ? t("Checking…") : t("Check project")}
        </Button>
        <Button
          className="primary"
          disabled={!ready || submitting}
          onClick={() => void submit()}
        >
          {submitting ? t("Starting…") : t("Export")}
        </Button>
      </div>
    </Modal>
  );
}
