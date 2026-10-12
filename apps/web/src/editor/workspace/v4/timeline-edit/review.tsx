import { useState } from "react";
import { Button, Modal } from "../../../shared/ui";
import { t, useLocale } from "../../../shared/locale";
import type { V4Sequence } from "../types";
import type { V4TimelineEdit } from "./controller";
import type { TimelineAttempt } from "./types";

/** Closing this window never cancels a dispatched write. Its original request
 * and transaction remain in the same Session's recovery state. */
export function TimelineEditReview({ attempt, controller, sequence, disabled, unavailableReason, onClose }: {
  attempt: TimelineAttempt;
  controller: V4TimelineEdit;
  sequence: V4Sequence;
  disabled: boolean;
  unavailableReason?: string | null;
  onClose(): void;
}) {
  useLocale();
  const [pending, setPending] = useState(false), [error, setError] = useState<string | null>(null);
  const run = async (action: () => Promise<unknown>) => {
    setPending(true); setError(null);
    try { await action(); } catch (e) { setError(e instanceof Error ? e.message : String(e)); }
    finally { setPending(false); }
  };
  const decision = attempt.plan?.decision;
  const names = new Map(sequence.tracks.flatMap(track => [[track.id, track.name],
    ...track.elements.map(element => [element.id, element.name])]) as [string, string][]);
  for (const operation of attempt.plan?.transaction?.operations ?? []) {
    if (operation.type === "element_put") names.set(operation.element.id, operation.element.name);
    if (operation.type === "track_put") names.set(operation.track.id, operation.track.name);
  }
  const groups: [string, string[]][] = decision ? [
    ["Created items", decision.created_ids], ["Changed items", decision.changed_ids],
    ["Deleted items", decision.deleted_ids], ["Moved by ripple", decision.ripple_shifted_ids],
    ["Removed transitions", decision.removed_transition_ids],
  ] : [];
  const status = attempt.phase === "planning" ? "Preparing timeline changes…"
    : attempt.phase === "applying" ? "Saving timeline changes…"
      : attempt.phase === "saved" ? "Timeline changes saved."
        : attempt.phase === "unconfirmed" ? "Timeline save is unconfirmed. Retry the original operation to recover its receipt."
          : attempt.phase === "conflict" ? "The original timeline operation was not saved. Discard it before making a new plan."
            : attempt.plan?.transaction === null ? "The plan makes no changes."
              : "Review timeline changes before saving.";
  const busy = pending || disabled || ["planning", "applying"].includes(attempt.phase);
  const discardable = !["planning", "applying", "unconfirmed"].includes(attempt.phase);
  return (
    <Modal title={t("Review timeline changes")} onClose={onClose}>
      <div className="padded scroll">
        <p role="status">{t(status)}</p>
        <p>{t("Operation")}: {t(attempt.request.label)}</p>
        {decision && <>
          <dl>
            <dt>{t("Created items")}</dt><dd>{decision.created_ids.length}</dd>
            <dt>{t("Changed items")}</dt><dd>{decision.changed_ids.length}</dd>
            <dt>{t("Deleted items")}</dt><dd>{decision.deleted_ids.length}</dd>
            <dt>{t("Moved by ripple")}</dt><dd>{decision.ripple_shifted_ids.length}</dd>
            <dt>{t("Removed transitions")}</dt><dd>{decision.removed_transition_ids.length}</dd>
          </dl>
          {groups.map(([label, ids]) => ids.length > 0 && <div key={label}>
            <strong>{t(label)}</strong>
            <ul>{ids.map(id => <li key={id}>{names.get(id) ?? id}</li>)}</ul>
          </div>)}
          <p>{t("Selection after saving")}: {decision.selection.map(target => names.get(target.element_id) ?? target.element_id).join(", ") || "—"}</p>
        </>}
        {attempt.error && <p role="alert">{t(attempt.error)}</p>}
        {unavailableReason && <p role="alert">{t(unavailableReason)}</p>}
        {error && <p role="alert">{t(error)}</p>}
        {attempt.phase === "unconfirmed" && <p className="muted">{t("The project head is not a receipt. Retry sends the original transaction and key; it does not create a new plan.")}</p>}
        <div className="toolbar">
          <Button onClick={onClose}>{t("Close")}</Button>
          {attempt.phase === "planning" && <Button onClick={() => { controller.cancel(); onClose(); }}>{t("Cancel planning")}</Button>}
          {discardable && <Button disabled={pending} onClick={() => void run(async () => { await controller.discard(); onClose(); })}>{t(attempt.phase === "saved" ? "Done" : "Discard timeline changes")}</Button>}
          {attempt.phase === "failed" && !attempt.plan && <Button disabled={busy} onClick={() => void run(() => controller.retryPlan())}>{t("Retry timeline plan")}</Button>}
          {attempt.phase === "unconfirmed" && <Button disabled={busy} onClick={() => void run(() => controller.apply())}>{t("Retry original timeline operation")}</Button>}
          {attempt.phase === "saved" && attempt.error && <Button disabled={busy} onClick={() => void run(() => controller.refreshSaved())}>{t("Refresh saved timeline changes")}</Button>}
          {["planned", "failed"].includes(attempt.phase) && attempt.plan?.transaction &&
            <Button disabled={busy} onClick={() => void run(() => controller.apply())}>{t("Apply timeline changes")}</Button>}
        </div>
      </div>
    </Modal>
  );
}
