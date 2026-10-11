import { useState } from "react";
import { createPortal } from "react-dom";
import { Pencil } from "lucide-react";
import type { ProjectRef } from "../model/types";
import { useWorkspace } from "../workspace/context";
import { t, useLocale } from "../shared/locale";
import { Button, Field, Modal } from "../shared/ui";

export function RenameProject({
  project,
  compact = false,
  menu = false,
}: {
  project: ProjectRef;
  compact?: boolean;
  menu?: boolean;
}) {
  useLocale();
  const { controller, busy, historical } = useWorkspace();
  const [open, setOpen] = useState(false),
    [name, setName] = useState(project.name),
    [pending, setPending] = useState(false);
  const save = async () => {
    setPending(true);
    try {
      await controller.renameProject(project, name.trim());
      setOpen(false);
    } catch (error) {
      controller.report(error);
    } finally {
      setPending(false);
    }
  };
  return (
    <>
      <Button
        className={compact ? "project-rename" : "project-name"}
        aria-label={t("Rename project {name}", { name: project.name })}
        disabled={
          busy || (!!historical && !compact) || project.format !== "visual"
        }
        icon={compact ? <Pencil size={14} /> : undefined}
        onClick={() => {
          setName(project.name);
          setOpen(true);
        }}
      >
        {compact ? (menu ? t("Rename project") : null) : project.name}
      </Button>
      {open &&
        createPortal(
          <Modal title={t("Rename project")} onClose={() => setOpen(false)}>
            <form
              onSubmit={(e) => {
                e.preventDefault();
                void save();
              }}
            >
              <Field label={t("Project name")}>
                <input
                  required
                  autoFocus
                  maxLength={200}
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                />
              </Field>
              <div className="modal-footer">
                <Button onClick={() => setOpen(false)}>{t("Cancel")}</Button>
                <button
                  type="submit"
                  className="primary"
                  disabled={
                    pending || !name.trim() || name.trim() === project.name
                  }
                >
                  {t("Save")}
                </button>
              </div>
            </form>
          </Modal>,
          document.body,
        )}
    </>
  );
}
