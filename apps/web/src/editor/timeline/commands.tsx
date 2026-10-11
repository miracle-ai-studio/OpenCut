import { t, useLocale } from "../shared/locale";
import { useState } from "react";
import type { ToolOutput } from "../../lib/editor-sdk";
import type { Clip, Sequence, TimelineCommand } from "../model/types";
import { fromSeconds, addTime, compareTime } from "../model/time";
import { useWorkspace } from "../workspace/context";
import { Button, Field, Modal } from "../shared/ui";
export function TimelineCommands({
  sequence,
  clip,
  onClose,
}: {
  sequence: Sequence;
  clip?: Clip;
  onClose: () => void;
}) {
  useLocale();
  const { controller, api, snapshot, playhead } = useWorkspace();
  const [kind, setKind] = useState("insert_gap"),
    [amount, setAmount] = useState(1),
    [plan, setPlan] = useState<ToolOutput<"opencut_edit_timeline_plan"> | null>(
      null,
    ),
    [error, setError] = useState<string | null>(null),
    [pending, setPending] = useState(false);
  const preview = async () => {
    if (!api || !snapshot) return;
    setPending(true);
    setError(null);
    setPlan(null);
    try {
      const delta = fromSeconds(amount);
      let command: TimelineCommand;
      if (kind === "insert_gap")
        command = { kind, at: playhead, duration: delta };
      else if (kind === "ripple_delete") {
        if (!clip)
          throw new Error("Select a clip to remove its timeline interval.");
        command = { kind, range: clip.timeline_range };
      } else if (kind === "slip" || kind === "slide") {
        if (!clip) throw new Error("Select a clip first.");
        command =
          kind === "slip"
            ? { kind, clip_id: clip.id, source_delta: delta }
            : { kind, clip_id: clip.id, delta };
      } else {
        if (!clip) throw new Error("Select the outgoing clip first.");
        const track = sequence.tracks.find((track) =>
          track.clips.some((item) => item.id === clip.id),
        );
        const incoming = track?.clips.find(
          (item) =>
            compareTime(
              item.timeline_range.start,
              addTime(clip.timeline_range.start, clip.timeline_range.duration),
            ) === 0,
        );
        if (!incoming)
          throw new Error("This clip has no adjacent incoming clip.");
        command = {
          kind: "roll",
          outgoing_clip_id: clip.id,
          incoming_clip_id: incoming.id,
          delta,
        };
      }
      setPlan(
        await api.timeline(
          snapshot,
          sequence.id,
          kind.replaceAll("_", " "),
          command,
        ),
      );
    } catch (error) {
      setError(String(error));
    } finally {
      setPending(false);
    }
  };
  const apply = async () => {
    if (!api || !plan) return;
    setPending(true);
    try {
      await controller.write(() =>
        api.sdk.call("opencut_edit_v3_apply", plan.transaction),
      );
      onClose();
    } catch (error) {
      setError(String(error));
    } finally {
      setPending(false);
    }
  };
  return (
    <Modal title={t("Timeline edit")} onClose={onClose}>
      <Field label={t("Edit")}>
        <select
          disabled={pending}
          value={kind}
          onChange={(event) => {
            setKind(event.target.value);
            setPlan(null);
          }}
        >
          <option value="insert_gap">{t("Insert time at playhead")}</option>
          <option value="ripple_delete">
            {t("Ripple delete selected interval")}
          </option>
          <option value="slip">{t("Slip source")}</option>
          <option value="roll">{t("Roll next cut")}</option>
          <option value="slide">{t("Slide selected clip")}</option>
        </select>
      </Field>
      {kind !== "ripple_delete" && (
        <Field label={t("Seconds")}>
          <input
            disabled={pending}
            type="number"
            step=".01"
            value={amount}
            onChange={(event) => {
              setAmount(Number(event.target.value));
              setPlan(null);
            }}
          />
        </Field>
      )}
      {plan && (
        <div role="status">
          <p>
            {plan.changed_ids.length} {t("changed ·")}
            {plan.created_ids.length} {t("created ·")}
            {plan.deleted_ids.length} {t("removed")}
          </p>
          {plan.notes.map((note) => (
            <p key={note}>{note}</p>
          ))}
        </div>
      )}
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      <div className="modal-footer">
        <Button disabled={pending} onClick={() => void preview()}>
          {t("Review edit")}
        </Button>
        <Button
          className="primary"
          disabled={
            pending ||
            !plan ||
            plan.transaction.expected_revision !== snapshot?.revision
          }
          onClick={() => void apply()}
        >
          {t("Apply edit")}
        </Button>
      </div>
    </Modal>
  );
}
