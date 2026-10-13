import { useState } from "react";
import { useWorkspace } from "../../context";
import { Button } from "../../../shared/ui";
import { t, useLocale } from "../../../shared/locale";

export function FontRecoveryPanel() {
  useLocale();
  const { controller, v4, busy } = useWorkspace();
  const [pending, setPending] = useState(false);
  const recovery = v4?.fontRecovery;
  if (!recovery || recovery.phase === "idle") return null;
  const working =
    pending ||
    busy ||
    ["reading", "staging", "applying"].includes(recovery.phase);
  const run = async (action: () => Promise<unknown>) => {
    setPending(true);
    try {
      await action();
    } catch (error) {
      controller.report(error);
    } finally {
      setPending(false);
    }
  };
  return (
    <section
      aria-label={t("Recover original font import")}
      className="history-banner"
    >
      <p role="status">
        {t(
          recovery.receipt
            ? "Original font import saved."
            : recovery.phase === "reading"
              ? "Reading original font import recovery…"
              : "The original font import needs recovery before another change.",
        )}
      </p>
      {recovery.error && <p role="alert">{t(recovery.error)}</p>}
      {recovery.record && (
        <p className="muted">
          {t(
            "Retry sends the original request and key. The project head is not an edit receipt.",
          )}
        </p>
      )}
      <div className="toolbar">
        <Button
          disabled={working}
          onClick={() => void run(() => controller.v4.recoverEdit())}
        >
          {t("Refresh recovery")}
        </Button>
        {recovery.error && (
          <Button
            disabled={working}
            onClick={() => void run(() => controller.v4.recoverEdit(true))}
          >
            {t("Repair recovery pointer")}
          </Button>
        )}
        {recovery.record && !recovery.receipt && (
          <Button
            disabled={working || !!v4?.historical}
            onClick={() => void run(() => controller.v4.fontRecovery.retry())}
          >
            {t("Retry original font import")}
          </Button>
        )}
        {recovery.record && (
          <Button
            disabled={working}
            onClick={() => void run(() => controller.v4.fontRecovery.release())}
          >
            {t("Release resolved font import")}
          </Button>
        )}
        {!recovery.record && recovery.receipt && (
          <Button
            disabled={working}
            onClick={() => controller.v4.fontRecovery.dismissSaved()}
          >
            {t("Done")}
          </Button>
        )}
      </div>
    </section>
  );
}
