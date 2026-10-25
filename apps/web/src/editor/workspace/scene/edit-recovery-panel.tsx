import { ErrorNotice } from "../../shared/error-notice";
import { useState } from "react";
import { useEditingWorkspace as useWorkspace } from "../context";
import { Button } from "../../shared/ui";
import { t, useLocale } from "../../shared/locale";

/** Shared recovery review. Reading or closing a panel never sends a write. */
export function EditRecoveryPanel() {
  useLocale();
  const { controller, scene, busy } = useWorkspace(), [pending, setPending] = useState(false);
  const recovery = scene?.recovery;
  if (!recovery || recovery.phase === "idle" || (!recovery.error && (busy || ["reading", "staging", "applying"].includes(recovery.phase))) || (!recovery.record && !recovery.error && recovery.receipt && recovery.phase === "saved")) return null;
  const intent = recovery.record?.context.intent;
  const label = intent?.kind === "public_apply" ? intent.request.label
    : intent?.kind === "undo" ? "Undo project changes" : intent?.kind === "redo" ? "Redo project changes"
    : intent?.kind === "font_import" ? "Import font" : intent?.kind === "subtitle_generate" ? "Generate subtitles" : intent?.kind === "subtitle_import" ? "Import subtitles" : "Restore project revision";
  const working = pending || busy || ["reading", "staging", "applying"].includes(recovery.phase);
  const run = async (action: () => Promise<unknown>) => {
    setPending(true); try { await action(); } catch (error) { controller.report(error); } finally { setPending(false); }
  };
  return <section role="region" aria-label={t("Recover original edit")} className="history-banner">
    <p role="status">{t(recovery.receipt ? "Original edit saved." : recovery.phase === "reading" ? "Reading original edit recovery…" : "The original edit needs recovery before another change.")}</p>
    {intent && <p>{t("Operation")}: {t(label)}</p>}
    {recovery.error && <ErrorNotice error={recovery.error} />}
    {recovery.record && <p className="muted">{t("Retry sends the original request and key. The project head is not an edit receipt.")}</p>}
    <div className="toolbar">
      <Button disabled={working} onClick={() => void run(() => controller.scene.recoverEdit())}>{t("Refresh recovery")}</Button>
      {recovery.error && <Button disabled={working} onClick={() => void run(() => controller.scene.recoverEdit(true))}>{t("Repair recovery pointer")}</Button>}
      {recovery.record && !recovery.receipt && <Button disabled={working || !!scene?.historical} onClick={() => void run(() => controller.scene.retryOriginalEdit())}>{t("Retry original edit")}</Button>}
      {recovery.record && <Button disabled={working} onClick={() => void run(() => controller.scene.recovery.release())}>{t(recovery.receipt ? "Done" : "Release resolved edit")}</Button>}
      {!recovery.record && recovery.receipt && <Button disabled={working} onClick={() => controller.scene.recovery.dismissSaved()}>{t("Done")}</Button>}
    </div>
  </section>;
}
