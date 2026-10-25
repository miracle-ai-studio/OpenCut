import { useEffect, useRef, useState } from "react";
import { useWorkspace } from "../../context";
import { Button, Field } from "../../../shared/ui";
import { t } from "../../../shared/locale";
import { editRecoveryBlocked } from "../edit-recovery";
import type { ToolInput } from "../../../../lib/editor-sdk";

type Request = ToolInput<"opencut_edit_apply">;

/** CaptionV4 uses the same public transaction/recovery path as other V4 edits.
 * A form never reauthors its captured caption against an external revision. */
export function CaptionTextEditor({ captionId }: { captionId: string }) {
  const { scene, busy } = useWorkspace();
  const [editing, setEditing] = useState(false);
  const sequence = scene?.head.project.sequences.find(s => s.id === scene.sequenceId);
  const caption = sequence?.captions.find(c => c.id === captionId);
  const track = sequence?.tracks.find(tr => tr.id === caption?.track_id);
  if (editing) return <CaptionTextForm captionId={captionId} onClose={() => setEditing(false)} />;
  return <Button disabled={busy || !caption || !track || track.locked || !!scene?.historical || !!scene?.recovery?.record}
    onClick={() => setEditing(true)}>{t("Edit caption text")}</Button>;
}

function CaptionTextForm({ captionId, onClose }: { captionId: string; onClose(): void }) {
  const { controller, api, scene, busy } = useWorkspace();
  const [captured] = useState(() => {
    const state = controller.getSnapshot(), view = state.scene;
    return {
      api: state.api, projectId: view?.head.project.id, revision: view?.head.revision,
      sequenceId: view?.sequenceId,
      caption: structuredClone(view?.head.project.sequences.find(s => s.id === view.sequenceId)?.captions.find(c => c.id === captionId)),
    };
  });
  const [text, setText] = useState(captured.caption?.text ?? "");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const attempt = useRef<Request | null>(null), alive = useRef(true), flight = useRef(false);
  useEffect(() => {
    alive.current = true;
    return () => { alive.current = false; };
  }, []);
  const current = scene?.head.project.sequences.find(s => s.id === scene.sequenceId)?.captions.find(c => c.id === captionId);
  const track = scene?.head.project.sequences.find(s => s.id === scene.sequenceId)?.tracks.find(tr => tr.id === current?.track_id);
  const stale = api !== captured.api || scene?.head.project.id !== captured.projectId ||
    scene?.head.revision !== captured.revision || scene?.sequenceId !== captured.sequenceId ||
    JSON.stringify(current) !== JSON.stringify(captured.caption);
  const blocked = busy || pending || !track || track.locked || !!scene?.historical || !!scene?.gainDraft ||
    !!scene?.recovery?.record || !!scene?.recovery?.error || editRecoveryBlocked(scene?.recovery) ||
    controller.scene.recovery.blocked() ||
    ["applying", "unconfirmed"].includes(scene?.timelineEdit.attempt?.phase ?? "");
  async function save() {
    if (flight.current || blocked || stale || !captured.caption || !captured.api) return;
    if (!text.trim()) { setError(t("Enter caption text.")); return; }
    if (text === captured.caption.text) { onClose(); return; }
    flight.current = true; setPending(true); setError(null);
    try {
      const request = attempt.current ??= {
        project_id: captured.projectId!, expected_revision: captured.revision!,
        idempotency_key: crypto.randomUUID(), label: "Edit caption text",
        operations: [{ type: "caption_put", sequence_id: captured.sequenceId!,
          caption: { ...captured.caption, text } }],
      };
      await captured.api.sdk.call("opencut_edit_plan", request);
      if (!alive.current || controller.getSnapshot().api !== captured.api ||
          controller.getSnapshot().scene?.selectedId !== captionId)
        throw Error("The caption target changed. Reopen its controls.");
      await controller.scene.applyPlanned(request, captured.sequenceId!);
      if (alive.current) onClose();
    } catch (cause) {
      if (alive.current) setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      flight.current = false;
      if (alive.current) setPending(false);
    }
  }
  return <section aria-label={t("Edit caption text")}>
    <Field label={t("Caption text")}><textarea value={text} disabled={pending || !!attempt.current}
      onChange={event => setText(event.target.value)} /></Field>
    {stale && <p role="alert">{t("The caption changed in another revision. Cancel and reopen its controls.")}</p>}
    {error && <p role="alert">{error}</p>}
    <div className="toolbar">
      <Button disabled={blocked || stale} onClick={() => void save()}>{t("Save caption text")}</Button>
      <Button disabled={pending} onClick={onClose}>{t("Cancel")}</Button>
    </div>
  </section>;
}
