import { useEffect, useState } from "react";
import { useWorkspace } from "../context";
import { Button, Field } from "../../shared/ui";
import { t } from "../../shared/locale";

/** Sound-only transport and opt-in synchronized Classic media preview. */
export function V4AudioPreviewControls() {
  const { controller, v4, api } = useWorkspace();
  const preview = controller.v4.audioPreview;
  const snapshot = v4 && (v4.historical ?? v4.head);
  const identity = `${snapshot?.project.id}/${snapshot?.revision}/${v4?.sequenceId}`;
  const [rate, setRate] = useState(48000);
  useEffect(() => {
    void preview.capability();
    const departed = () => preview.retireDocument();
    window.addEventListener("pagehide", departed);
    return () => {
      window.removeEventListener("pagehide", departed);
      preview.invalidate();
    };
  }, [preview, api, identity]);
  const capability = v4?.audioPreview.capability;
  const rates =
    capability?.sample_rates.filter((r) => r === 44100 || r === 48000) ?? [];
  useEffect(() => {
    if (rates.length && !rates.some((r) => r === rate)) setRate(rates[0]);
  }, [capability, rate]);
  if (!v4) return null;
  const state = v4.audioPreview;
  const active = ["loading", "playing", "stalled"].includes(state.phase);
  const run = (action: () => Promise<unknown>) => {
    try {
      void action().catch((e) => controller.report(e));
    } catch (e) {
      controller.report(e);
    }
  };
  const status =
    state.phase === "loading"
      ? "Preparing scene sound…"
      : state.phase === "playing"
        ? "Playing scene sound"
        : state.phase === "stalled"
          ? "Waiting for scene audio; the playhead is stopped."
          : state.phase === "paused"
            ? "Scene sound paused"
            : state.phase === "ended"
              ? "Scene sound ended"
              : state.phase === "unconfirmed"
                ? "Audio session is unconfirmed. Retry the original request."
                : "Scene sound is stopped";
  return (
    <div className="v4-audio-preview padded" aria-label={t("Scene sound")}>
      <div className="toolbar">
        <Field label={t("Audio preview sample rate")}>
          <select
            value={rate}
            disabled={
              active || state.phase === "unconfirmed" || !capability?.available
            }
            onChange={(e) => setRate(Number(e.target.value))}
          >
            {rates.map((r) => (
              <option key={r} value={r}>
                {r} Hz
              </option>
            ))}
          </select>
        </Field>
        <Button
          disabled={
            !capability?.available || active || state.phase === "unconfirmed"
          }
          onClick={() => run(() => preview.play(rate))}
        >
          {t("Play scene audio")}
        </Button>
        <Button
          disabled={
            !capability?.available || active || state.phase === "unconfirmed"
          }
          onClick={() =>
            run(() => preview.play(rate, controller.v4.visualPreview.start()))
          }
        >
          {t("Play scene preview")}
        </Button>
        <Button disabled={!active} onClick={() => run(() => preview.pause())}>
          {t("Pause scene audio")}
        </Button>
        {state.phase === "unconfirmed" && (
          <Button onClick={() => run(() => preview.retry())}>
            {t("Retry original audio session")}
          </Button>
        )}
      </div>
      <p role="status">{t(status)}</p>
      <p className="muted">
        {t(
          "Scene preview follows confirmed sound consumption. Video, static PNG/JPEG, fixed-font text, captions and basic graphics support the available blend modes. Rectangular and elliptical feathered masks are supported on media and graphics. Complex masks and effects require separate rendering support.",
        )}
      </p>
      {!capability?.available && (
        <p>
          {t(
            capability?.unavailable_reason ??
              state.error ??
              "This service does not support live scene audio.",
          )}
        </p>
      )}
      {state.error && capability?.available && (
        <p role="alert">{t(state.error)}</p>
      )}
    </div>
  );
}
