import { useMediaPreview } from "../workspace/media-preview";
import { MediaArt } from "../shared/media-art";
import { t, useLocale } from "../shared/locale";
import { useEffect, useRef, useState } from "react";
import {
  Film,
  Image,
  Music2,
  Plus,
  Search,
  Upload,
  X,
  RefreshCw,
  LayoutGrid,
  List,
  Trash2,
} from "lucide-react";
import { useWorkspace } from "../workspace/context";
import { activeSequence } from "../model/types";
import { formatTime } from "../model/time";
import { Button, Empty, Panel, Modal } from "../shared/ui";
import { SourcePreview } from "./source-preview";
import type { Asset } from "../model/types";
import { uploadMedia } from "./upload";
import { placeAsset } from "../model/edit-requests";
import { sortMedia, type MediaSort } from "../model/media-library";
export function AssetsPanel({
  audioOnly = false,
  active = true,
}: {
  audioOnly?: boolean;
  active?: boolean;
}) {
  useLocale();
  const { controller, api, snapshot, historical, sequenceId, busy } =
    useWorkspace();
  const [search, setSearch] = useState(""),
    [progress, setProgress] = useState<string | null>(null);
  const input = useRef<HTMLInputElement>(null),
    abort = useRef<AbortController | null>(null);
  const [source, setSource] = useState<Asset | null>(null);
  const [layout, setLayout] = useState("grid"),
    [sort, setSort] = useState<MediaSort>("name"),
    [ascending, setAscending] = useState(true),
    [selected, setSelected] = useState<string[]>([]),
    [deleting, setDeleting] = useState(false);
  const anchor = useRef<string | null>(null),
    shift = useRef(false);
  const replacement = useRef<Asset | null>(null);
  const chooseFiles = (asset: Asset | null = null) => {
    replacement.current = asset;
    if (input.current) {
      input.current.multiple = !asset;
      input.current.click();
    }
  };
  const projectId = snapshot?.project.id;
  useEffect(() => () => abort.current?.abort(), [projectId, api]);
  const importFiles = async (files: FileList | File[] | null) => {
    if (!files || !api || !snapshot || progress || historical || abort.current)
      return;
    const request = new AbortController();
    abort.current = request;
    const replacing = replacement.current;
    try {
      for (const file of files) {
        setProgress(t("Importing {name}", { name: file.name }));
        const imported = await uploadMedia(
          api.sdk,
          snapshot.project.id,
          file,
          setProgress,
          request.signal,
        );
        const current = controller.getSnapshot();
        if (
          current.api !== api ||
          current.snapshot?.project.id !== snapshot.project.id
        )
          return;
        if (replacing && replacing.kind !== imported.asset.kind)
          throw new Error(
            "Replacement media must have the same type as the original.",
          );
        await controller.mutate(
          replacing ? `Replace ${replacing.name}` : `Import ${file.name}`,
          [
            replacing
              ? {
                  type: "asset.replace",
                  asset: { ...imported.asset, id: replacing.id },
                }
              : { type: "asset.bind", asset: imported.asset },
          ],
        );
        if (imported.warning) controller.notify(imported.warning);
      }
    } catch (error) {
      controller.report(error);
    } finally {
      if (abort.current === request) abort.current = null;
      replacement.current = null;
      setProgress(null);
      if (input.current) {
        input.current.value = "";
        input.current.multiple = true;
      }
    }
  };
  useEffect(() => {
    if (!active) return;
    const paste = (event: ClipboardEvent) => {
      if (
        !event.clipboardData?.files.length ||
        (event.target instanceof HTMLElement &&
          event.target.closest(
            "input,textarea,select,[contenteditable=true],dialog",
          ))
      )
        return;
      event.preventDefault();
      replacement.current = null;
      void importFiles(event.clipboardData.files);
    };
    document.addEventListener("paste", paste);
    return () => document.removeEventListener("paste", paste);
  });
  useEffect(() => {
    setSelected([]);
    anchor.current = null;
    setSource(null);
  }, [projectId]);
  const view = historical ?? snapshot,
    sequence = activeSequence(view, sequenceId),
    assets = sortMedia(
      view?.project.assets.filter(
        (asset) =>
          asset.name.toLowerCase().includes(search.toLowerCase()) &&
          (!audioOnly || asset.kind === "audio"),
      ) ?? [],
      sort,
      ascending,
    );
  const chosen =
    view?.project.assets.filter((a) => selected.includes(a.id)) ?? [];
  const remove = async () => {
    if (!snapshot || !chosen.length) return;
    try {
      await controller.removeMedia(chosen.map((a) => a.id));
      setSelected([]);
      setDeleting(false);
      setSource(null);
    } catch (error) {
      controller.report(error);
    }
  };
  return (
    <Panel
      title={t(audioOnly ? "Sounds" : "Media")}
      actions={
        <Button
          aria-label={t("Import media")}
          disabled={!snapshot || !!historical || !!progress}
          icon={<Upload size={15} />}
          onClick={() => chooseFiles()}
        />
      }
    >
      <input
        ref={input}
        aria-label={t("Media files")}
        className="hidden"
        type="file"
        multiple
        accept={audioOnly ? "audio/*" : "video/*,audio/*,image/*"}
        onChange={(event) => void importFiles(event.target.files)}
      />
      <div className="search">
        <Search size={14} />
        <input
          aria-label={t("Search media")}
          placeholder={t("Search media")}
          value={search}
          onChange={(event) => setSearch(event.target.value)}
        />
      </div>
      <div className="media-library-toolbar toolbar">
        <select
          aria-label={t("Sort media")}
          value={sort}
          onChange={(e) => setSort(e.target.value as MediaSort)}
        >
          <option value="name">{t("Name")}</option>
          <option value="type">{t("Type")}</option>
          <option value="duration">{t("Duration")}</option>
          <option value="size">{t("Size")}</option>
        </select>
        <Button
          aria-label={t(
            ascending ? "Sort media ascending" : "Sort media descending",
          )}
          onClick={() => setAscending((value) => !value)}
        >
          {ascending ? "↑" : "↓"}
        </Button>
        <Button
          aria-label={t("Media grid view")}
          aria-pressed={layout === "grid"}
          icon={<LayoutGrid size={14} />}
          onClick={() => setLayout("grid")}
        />
        <Button
          aria-label={t("Media list view")}
          aria-pressed={layout === "list"}
          icon={<List size={14} />}
          onClick={() => setLayout("list")}
        />
      </div>
      {!!chosen.length && (
        <div className="media-selection-toolbar toolbar">
          <span>{t("{count} selected", { count: chosen.length })}</span>
          <Button
            className="danger"
            aria-label={t("Remove selected media")}
            disabled={busy || !!historical || !!progress || chosen.length > 64}
            icon={<Trash2 size={14} />}
            onClick={() => setDeleting(true)}
          />
          <Button
            onClick={() => {
              setSelected([]);
              anchor.current = null;
            }}
          >
            {t("Clear selection")}
          </Button>
        </div>
      )}
      {progress && (
        <div className="upload-progress" role="status">
          {progress}
          <Button
            aria-label={t("Cancel import")}
            icon={<X size={14} />}
            onClick={() => abort.current?.abort()}
          />
        </div>
      )}
      <div
        className={`asset-grid scroll ${layout === "list" ? "asset-list" : ""}`}
        onDragOver={(event) => {
          if (
            !historical &&
            !progress &&
            event.dataTransfer.types.includes("Files")
          )
            event.preventDefault();
        }}
        onDrop={(event) => {
          if (event.dataTransfer.files.length) {
            event.preventDefault();
            replacement.current = null;
            void importFiles(event.dataTransfer.files);
          }
        }}
      >
        {!assets.length && (
          <Empty title={t("Bring your story in")}>
            {t("Drop video, images or audio here.")}
            <Button onClick={() => chooseFiles()} disabled={!!historical}>
              {t("Import media")}
            </Button>
          </Empty>
        )}
        {assets.map((asset) => (
          <article
            key={asset.id}
            className={`asset-card ${asset.kind} ${selected.includes(asset.id) ? "selected" : ""}`}
            draggable={!historical}
            onDragStart={(event) =>
              event.dataTransfer.setData("application/opencut-asset", asset.id)
            }
          >
            <input
              className="asset-select"
              type="checkbox"
              aria-label={t("Select media {name}", { name: asset.name })}
              checked={selected.includes(asset.id)}
              onClickCapture={(e) => {
                shift.current = e.shiftKey;
              }}
              onChange={(e) => {
                const checked = e.target.checked,
                  range = shift.current,
                  previous = anchor.current,
                  a = assets.findIndex((x) => x.id === previous),
                  b = assets.findIndex((x) => x.id === asset.id);
                setSelected((ids) =>
                  checked
                    ? range && a >= 0
                      ? [
                          ...new Set([
                            ...ids,
                            ...assets
                              .slice(Math.min(a, b), Math.max(a, b) + 1)
                              .map((x) => x.id),
                          ]),
                        ]
                      : [...new Set([...ids, asset.id])]
                    : ids.filter((id) => id !== asset.id),
                );
                if (checked && (!range || a < 0)) anchor.current = asset.id;
                shift.current = false;
              }}
            />
            <button
              className="asset-poster"
              aria-label={t("Preview {value0}", { value0: asset.name })}
              onClick={() => setSource(asset)}
            >
              {asset.kind === "audio" ? (
                <Music2 />
              ) : asset.kind === "image" ? (
                <Image />
              ) : (
                <Film />
              )}
              <AssetArt asset={asset} />
              <span>
                {asset.duration ? formatTime(asset.duration) : asset.kind}
              </span>
            </button>
            <div className="asset-caption">
              <span title={asset.name}>{asset.name}</span>
              <Button
                aria-label={t("Remove {name}", { name: asset.name })}
                disabled={busy || !!historical || !!progress}
                icon={<Trash2 size={12} />}
                onClick={() => {
                  setSelected([asset.id]);
                  setDeleting(true);
                }}
              />
              <Button
                aria-label={t("Replace {value0}", { value0: asset.name })}
                disabled={busy || !!historical || !!progress}
                icon={<RefreshCw size={12} />}
                onClick={() => chooseFiles(asset)}
              />
              <Button
                aria-label={t("Add {value0} to timeline", {
                  value0: asset.name,
                })}
                disabled={busy || !!historical}
                icon={<Plus size={14} />}
                onClick={() => {
                  if (sequence)
                    void controller
                      .mutate(`Add ${asset.name}`, placeAsset(asset, sequence))
                      .catch((error) => controller.report(error));
                }}
              />
            </div>
          </article>
        ))}
      </div>
      {source && (
        <SourcePreview
          key={source.id}
          asset={source}
          onClose={() => setSource(null)}
        />
      )}
      {deleting && (
        <Modal title={t("Remove media")} onClose={() => setDeleting(false)}>
          <p>
            {t(
              "Remove {count} media items and their clips from all scenes? You can undo this change.",
              { count: chosen.length },
            )}
          </p>
          <ul className="delete-project-names">
            {chosen.map((a) => (
              <li key={a.id}>{a.name}</li>
            ))}
          </ul>
          <div className="modal-footer">
            <Button onClick={() => setDeleting(false)}>{t("Cancel")}</Button>
            <Button
              className="danger"
              disabled={busy || !chosen.length}
              onClick={() => void remove()}
            >
              {t("Remove media")}
            </Button>
          </div>
        </Modal>
      )}
    </Panel>
  );
}

function AssetArt({ asset }: { asset: Asset | undefined }) {
  const art = useMediaPreview(asset);
  return <MediaArt {...art} />;
}
