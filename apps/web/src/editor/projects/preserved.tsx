import { useMemo, useState } from "react";
import { Download, LockKeyhole, RefreshCw } from "lucide-react";
import { OpenCutApiError, type ToolInput } from "../../lib/editor-sdk";
import { useWorkspace } from "../workspace/context";
import { Button, Panel } from "../shared/ui";
import { t, useLocale } from "../shared/locale";

export function PreservedProject() {
  useLocale();
  const { api, preserved, controller } = useWorkspace();
  const [pending, setPending] = useState(false);
  const [exportMessage, setExportMessage] = useState<string | null>(null);
  const request = useMemo<ToolInput<"opencut_render_submit"> | null>(
    () =>
      preserved
        ? {
            kind: "video",
            request: {
              project_id: preserved.project_id,
              revision: preserved.revision,
              sequence_id: preserved.known_content.default_sequence_id,
              idempotency_key: crypto.randomUUID(),
              options: { format: "mp4", quality: "high", include_audio: true },
            },
          }
        : null,
    [preserved?.project_id, preserved?.revision],
  );
  if (!api || !preserved || !request) return null;
  const download = async () => {
    setPending(true);
    try {
      const file = await api.sdk.readOnlyProjectManifest({
        project_id: preserved.project_id,
        revision: preserved.revision,
      });
      if (
        file.revision !== preserved.revision ||
        file.project_id !== preserved.project_id
      )
        throw new Error("Preservation file identity changed.");
      const url = URL.createObjectURL(
        new Blob([JSON.stringify(file, null, 2) + "\n"], {
          type: "application/json",
        }),
      );
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = "opencut-preserved-project.json";
      anchor.click();
      setTimeout(() => URL.revokeObjectURL(url), 0);
    } catch (error) {
      controller.report(error);
    } finally {
      setPending(false);
    }
  };
  const exportVideo = async () => {
    setPending(true);
    setExportMessage(null);
    try {
      await api.sdk.call("opencut_render_submit", request);
      controller.report(
        new Error(
          "Export was unexpectedly accepted; inspect its task before retrying.",
        ),
      );
    } catch (error) {
      if (
        error instanceof OpenCutApiError &&
        error.code === "MISSING_PLUGIN_DEPENDENCY"
      )
        setExportMessage(
          t(
            "Export is blocked because required effect plugins are unavailable. Your project data is preserved.",
          ),
        );
      else controller.report(error);
    } finally {
      setPending(false);
    }
  };
  const reasons = {
    not_installed: t("Plugin is not installed"),
    unsupported_by_host: t("This version cannot run this effect plugin"),
    invalid_package: t("The installed plugin does not match this project"),
    inspection_unavailable: t("Plugin availability could not be checked"),
  };
  return (
    <main
      className="preserved-workspace"
      aria-label={t("Read-only project")}
      data-project-id={preserved.project_id}
      data-revision={preserved.revision}
    >
      <Panel
        title={t("Read-only project")}
        actions={
          <>
            <Button
              disabled={pending}
              onClick={() =>
                void controller.open({
                  id: preserved.project_id,
                  name: preserved.name,
                  revision: preserved.revision,
                  format: "readonly",
                })
              }
              icon={<RefreshCw size={14} />}
            >
              {t("Refresh")}
            </Button>
            <Button
              disabled={pending}
              onClick={() => void download()}
              icon={<Download size={14} />}
            >
              {t("Download project data")}
            </Button>
            <Button disabled={pending} onClick={() => void exportVideo()}>
              {t("Export video")}
            </Button>
          </>
        }
      >
        <h1>
          <LockKeyhole size={22} /> {preserved.name}
        </h1>
        <p>
          {t(
            "Unknown effect data is preserved. Editing and export require compatible effect support.",
          )}
        </p>
        <p className="muted">
          {t("{count} scenes", {
            count: preserved.known_content.sequences.length,
          })}{" "}
          ·{" "}
          {t("{count} media assets", {
            count: preserved.known_content.assets.length,
          })}
        </p>
        {exportMessage && (
          <p role="alert" className="notice">
            {exportMessage}
          </p>
        )}
      </Panel>
      <Panel title={t("Unavailable effects")}>
        <div className="preserved-effects">
          {preserved.effects.map((effect) => {
            const scene = preserved.known_content.sequences.find(
              (s) => s.id === effect.target.sequence_id,
            );
            const element = scene?.tracks
              .find((track) => track.id === effect.target.track_id)
              ?.elements.find((e) => e.id === effect.target.element_id);
            return (
              <article
                key={effect.id}
                data-effect-id={effect.id}
                data-payload-sha256={effect.payload_sha256}
              >
                <strong>
                  {effect.plugin.id} · {effect.plugin.version}
                </strong>
                <p>
                  {scene?.name} · {element?.name}
                </p>
                <p>{reasons[effect.unavailable_reason]}</p>
                <small>{t("Configuration preserved")}</small>
              </article>
            );
          })}
        </div>
      </Panel>
    </main>
  );
}
