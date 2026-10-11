import { useRef, useState } from "react";
import { createPortal } from "react-dom";
import type { ProjectRef } from "../model/types";
import { useWorkspace } from "../workspace/context";
import { t, useLocale } from "../shared/locale";
import { Button, Field, Modal } from "../shared/ui";

export function DeleteProjects({
  projects,
  onClose,
}: {
  projects: ProjectRef[];
  onClose: () => void;
}) {
  useLocale();
  const { controller } = useWorkspace();
  const idempotencyKey = useRef(crypto.randomUUID());
  const [removed, setRemoved] = useState(false);
  const [confirmation, setConfirmation] = useState(""),
    [pending, setPending] = useState(false),
    [error, setError] = useState<string | null>(null);
  const remove = async () => {
    if ((!removed && confirmation !== "DELETE") || pending) return;
    setPending(true);
    setError(null);
    try {
      const result = await controller.deleteProjects(
        projects,
        idempotencyKey.current,
      );
      setRemoved(true);
      if (result.cleanup_pending.length)
        setError(
          "Projects were deleted. Some temporary files could not be removed. Retry cleanup.",
        );
      else onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setPending(false);
    }
  };
  return createPortal(
    <Modal
      title={t("Delete projects")}
      onClose={() => {
        if (!pending) onClose();
      }}
    >
      <form
        onSubmit={(e) => {
          e.preventDefault();
          void remove();
        }}
      >
        <p>
          {t("Permanently delete {count} projects? This cannot be undone.", {
            count: projects.length,
          })}
        </p>
        <ul className="delete-project-names">
          {projects.map((p) => (
            <li key={p.id}>{p.name}</li>
          ))}
        </ul>
        {!removed && (
          <Field label={t('Type "DELETE" to confirm')}>
            <input
              autoFocus
              value={confirmation}
              disabled={pending}
              autoComplete="off"
              onChange={(e) => setConfirmation(e.target.value)}
              placeholder="DELETE"
            />
          </Field>
        )}
        {error && (
          <p role="alert" className="error">
            {t(error)}
          </p>
        )}
        <div className="modal-footer">
          <Button disabled={pending} onClick={onClose}>
            {t("Cancel")}
          </Button>
          <button
            type="submit"
            className="danger"
            disabled={pending || (!removed && confirmation !== "DELETE")}
          >
            {t(
              pending
                ? "Deleting…"
                : removed
                  ? "Retry cleanup"
                  : "Delete projects",
            )}
          </button>
        </div>
      </form>
    </Modal>,
    document.body,
  );
}
