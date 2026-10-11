import { useEffect, useState } from "react";
import { Check, Layers3, Pencil, Plus, Trash2 } from "lucide-react";
import { useWorkspace } from "../workspace/context";
import { t, useLocale } from "../shared/locale";
import { Button, Field, Modal } from "../shared/ui";

export function SceneSwitcher() {
  useLocale();
  const { controller, snapshot, historical, sequenceId, busy } = useWorkspace();
  const [open, setOpen] = useState(false),
    [selected, setSelected] = useState<string[]>([]);
  const [editing, setEditing] = useState<{
      id: string | null;
      name: string;
    } | null>(null),
    [confirming, setConfirming] = useState(false);
  const view = historical ?? snapshot;
  const project = view?.project;
  const editable =
    !!snapshot && snapshot.format === "visual" && !historical && !busy;
  useEffect(() => {
    setSelected([]);
    setEditing(null);
    setConfirming(false);
    setOpen(false);
  }, [project?.id]);
  useEffect(() => {
    if (!snapshot || !sequenceId || !location.pathname.startsWith("/editor/"))
      return;
    const url = new URL(location.href);
    url.searchParams.set("scene", sequenceId);
    history.replaceState(null, "", url.pathname + url.search);
  }, [snapshot?.project.id, sequenceId]);
  if (!project) return null;
  const save = async () => {
    if (!editing || !editable) return;
    const id = editing.id ?? crypto.randomUUID(),
      projectId = project.id;
    try {
      await controller.editProject(
        editing.id ? "Rename scene" : "Create scene",
        [
          editing.id
            ? {
                type: "rename_scene",
                sequence_id: id,
                name: editing.name.trim(),
              }
            : {
                type: "create_scene",
                sequence_id: id,
                name: editing.name.trim(),
                video_track_id: crypto.randomUUID(),
              },
        ],
      );
      if (controller.getSnapshot().snapshot?.project.id !== projectId) return;
      setEditing(null);
      if (!editing.id && controller.getSnapshot().sequenceId === sequenceId)
        controller.chooseSequence(id);
    } catch (error) {
      controller.report(error);
    }
  };
  const remove = async () => {
    try {
      await controller.editProject(
        "Delete scenes",
        selected.map((sequence_id) => ({ type: "delete_scene", sequence_id })),
      );
      setSelected([]);
      setConfirming(false);
    } catch (error) {
      controller.report(error);
    }
  };
  return (
    <div className="toolbar">
      <Layers3 size={13} />
      <select
        aria-label={t("Active sequence")}
        value={sequenceId ?? ""}
        onChange={(e) => controller.chooseSequence(e.target.value)}
      >
        {project.sequences.map((s) => (
          <option key={s.id} value={s.id}>
            {s.name}
          </option>
        ))}
      </select>
      <Button
        aria-label={t("Manage scenes")}
        icon={<Layers3 size={14} />}
        onClick={() => setOpen(true)}
      >
        {t("Scenes")}
      </Button>
      {open && (
        <Modal
          className="scene-manager"
          title={t("Scenes")}
          onClose={() => setOpen(false)}
        >
          <p className="muted">{t("Switch between scenes in your project")}</p>
          <div className="toolbar scene-actions">
            <Button
              icon={<Plus size={14} />}
              disabled={!editable}
              onClick={() => {
                setEditing({
                  id: null,
                  name: t("Scene {number}", {
                    number: project.sequences.length + 1,
                  }),
                });
                setConfirming(false);
              }}
            >
              {t("New scene")}
            </Button>
            <Button
              icon={<Trash2 size={14} />}
              disabled={!editable || !selected.length}
              onClick={() => {
                setConfirming(true);
                setEditing(null);
              }}
            >
              {t("Delete selected ({count})", { count: selected.length })}
            </Button>
          </div>
          {editing && (
            <form
              className="scene-form"
              onSubmit={(e) => {
                e.preventDefault();
                void save();
              }}
            >
              <Field label={t("Scene name")}>
                <input
                  required
                  autoFocus
                  maxLength={200}
                  value={editing.name}
                  onChange={(e) =>
                    setEditing({ ...editing, name: e.target.value })
                  }
                />
              </Field>
              <div className="toolbar">
                <button
                  type="submit"
                  className="primary"
                  disabled={
                    !editable ||
                    !editing.name.trim() ||
                    (!!editing.id &&
                      project.sequences.find((s) => s.id === editing.id)
                        ?.name === editing.name.trim())
                  }
                >
                  {t(editing.id ? "Save" : "Create scene")}
                </button>
                <Button onClick={() => setEditing(null)}>{t("Cancel")}</Button>
              </div>
            </form>
          )}
          {confirming && (
            <div className="scene-confirm" role="alert">
              <p>
                {t("Delete {count} scenes and their timeline contents?", {
                  count: selected.length,
                })}
              </p>
              <p className="muted">
                {t(
                  "Your media stays in the library. You can undo this change.",
                )}
              </p>
              <div className="toolbar">
                <Button
                  className="danger"
                  disabled={!editable}
                  onClick={() => void remove()}
                >
                  {t("Delete scenes")}
                </Button>
                <Button onClick={() => setConfirming(false)}>
                  {t("Cancel")}
                </Button>
              </div>
            </div>
          )}
          <div className="scene-list">
            {project.sequences.map((scene) => {
              const main = scene.id === project.default_sequence_id;
              return (
                <div
                  className={`scene-row ${scene.id === sequenceId ? "active" : ""}`}
                  key={scene.id}
                >
                  <input
                    type="checkbox"
                    aria-label={t("Select scene {name}", { name: scene.name })}
                    disabled={!editable || main}
                    checked={selected.includes(scene.id)}
                    onChange={(e) => {
                      setConfirming(false);
                      setSelected(
                        e.target.checked
                          ? [...selected, scene.id]
                          : selected.filter((id) => id !== scene.id),
                      );
                    }}
                  />
                  <Button
                    className="scene-choice"
                    onClick={() => controller.chooseSequence(scene.id)}
                    aria-pressed={scene.id === sequenceId}
                  >
                    <span>
                      <strong>{scene.name}</strong>
                      <small>{main ? t("Main scene") : t("Scene")}</small>
                    </span>
                    {scene.id === sequenceId && <Check size={15} />}
                  </Button>
                  <Button
                    aria-label={t("Rename scene {name}", { name: scene.name })}
                    icon={<Pencil size={14} />}
                    disabled={!editable}
                    onClick={() => {
                      setEditing({ id: scene.id, name: scene.name });
                      setConfirming(false);
                    }}
                  />
                </div>
              );
            })}
          </div>
        </Modal>
      )}
    </div>
  );
}
