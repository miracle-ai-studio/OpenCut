import { fontRecoveryBlocked } from "./text/font-recovery";
import { FontRecoveryPanel } from "./text/font-recovery-panel";
import { useState } from "react";
import { useWorkspace } from "../context";
import { Button } from "../../shared/ui";
import { t, useLocale } from "../../shared/locale";

/** Shared recovery review. Reading or closing a panel never sends a write. */
export function EditRecoveryPanel() {
  useLocale();
  const { controller, v4, busy } = useWorkspace(), [pending, setPending] = useState(false);
  const recovery = v4?.recovery;
  if (v4?.fontRecovery && v4.fontRecovery.phase !== "idle" && (fontRecoveryBlocked(v4.fontRecovery) || !recovery || recovery.phase === "idle")) return <FontRecoveryPanel />;
  if (!recovery || recovery.phase === "idle") return null;
  const intent = recovery.record?.context.intent;
  const label = intent?.kind === "public_apply" ? intent.request.label
    : intent?.kind === "undo" ? "Undo project changes" : intent?.kind === "redo" ? "Redo project changes" : "Restore project revision";
  const working = pending || busy || ["reading", "staging", "applying"].includes(recovery.phase);
  const run = async (action: () => Promise<unknown>) => {
    setPending(true); try { await action(); } catch (error) { controller.report(error); } finally { setPending(false); }
  };
  return <section role="region" aria-label={t("Recover original edit")} className="history-banner">
    <p role="status">{t(recovery.receipt ? "Original edit saved." : recovery.phase === "reading" ? "Reading original edit recovery…" : "The original edit needs recovery before another change.")}</p>
    {intent && <p>{t("Operation")}: {t(label)}</p>}
    {recovery.error && <p role="alert">{t(recovery.error)}</p>}
    {recovery.record && <p className="muted">{t("Retry sends the original request and key. The project head is not an edit receipt.")}</p>}
    <div className="toolbar">
      <Button disabled={working} onClick={() => void run(() => controller.v4.recoverEdit())}>{t("Refresh recovery")}</Button>
      {recovery.error && <Button disabled={working} onClick={() => void run(() => controller.v4.recoverEdit(true))}>{t("Repair recovery pointer")}</Button>}
      {recovery.record && !recovery.receipt && <Button disabled={working || !!v4?.historical} onClick={() => void run(() => controller.v4.retryOriginalEdit())}>{t("Retry original edit")}</Button>}
      {recovery.record && <Button disabled={working} onClick={() => void run(() => controller.v4.recovery.release())}>{t("Release resolved edit")}</Button>}
      {!recovery.record && recovery.receipt && <Button disabled={working} onClick={() => controller.v4.recovery.dismissSaved()}>{t("Done")}</Button>}
    </div>
  </section>;
}
