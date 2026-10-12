import { useState } from "react";
import type { Snapshot } from "../model/types";
import { deleteTrack } from "../model/selection";
import { t, useLocale } from "../shared/locale";
import { Button, Modal } from "../shared/ui";
import { useWorkspace } from "../workspace/context";

export function DeleteTrack({
  view,
  sequenceId,
  trackId,
  onClose,
}: {
  view: Snapshot;
  sequenceId: string;
  trackId: string;
  onClose: () => void;
}) {
  useLocale();
  const {
    controller,
    snapshot,
    sequenceId: currentSequence,
    busy,
    historical,
  } = useWorkspace();
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const track = view.project.sequences
    .find((scene) => scene.id === sequenceId)
    ?.tracks.find((track) => track.id === trackId);
  const shapeCount =
    view.visual
      .find((graph) => graph.sequence_id === sequenceId)
      ?.shapes.filter((shape) => shape.track_id === trackId).length ?? 0;
  const stale =
    snapshot?.project.id !== view.project.id ||
    snapshot.revision !== view.revision ||
    currentSequence !== sequenceId ||
    !!historical;
  const remove = async () => {
    if (stale || busy || pending) return;
    setPending(true);
    setError(null);
    try {
      await controller.mutate(
        "Delete track",
        [],
        deleteTrack(view, sequenceId, trackId),
        view.revision,
      );
      onClose();
    } catch (error) {
      setError(error instanceof Error ? error.message : String(error));
    } finally {
      setPending(false);
    }
  };
  return (
    <Modal title={t("Delete track")} onClose={onClose}>
      <p>
        {t("Delete {name} and its {count} elements?", {
          name: track?.name ?? trackId,
          count: (track?.clips.length ?? 0) + shapeCount,
        })}
      </p>
      <p>
        {t(
          "Related effects, masks, transitions and animation will also be removed. You can undo this edit. Media stays in the library.",
        )}
      </p>
      {stale && (
        <p role="status">
          {t(
            "The project changed. Close this dialog and review the track again.",
          )}
        </p>
      )}
      {error && (
        <p className="error" role="alert">
          {t(error)}
        </p>
      )}
      <div className="modal-footer">
        <Button onClick={onClose}>{t("Cancel")}</Button>
        <Button
          className="danger"
          disabled={stale || busy || pending || !track || track.locked}
          onClick={() => void remove()}
        >
          {t("Delete track")}
        </Button>
      </div>
    </Modal>
  );
}
