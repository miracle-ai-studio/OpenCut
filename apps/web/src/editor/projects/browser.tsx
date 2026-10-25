import "./browser.css";
import { Select } from "../shared/controls";
import { RestoreProjectPackage } from "../workspace/scene/package";
import { RenameProject } from "./rename";
import { ProjectCover } from "./cover";
import { DeleteProjects } from "./delete";
import { t, useLocale } from "../shared/locale";
import { useEntrance } from "../shared/motion";
import { useEffect, useRef, useState } from "react";
import { copyNames, sortProjects, selectRange, type SortKey } from "./library";
import {
  readLibraryPreferences,
  saveLibraryPreferences,
} from "../shared/view-preferences";
import { ProjectInfo, projectDate } from "./info";
import { formatTime } from "../model/time";
import type { ProjectRef } from "../model/types";
import {
  FolderOpen,
  Plus,
  RefreshCw,
  Search,
  LayoutGrid,
  List,
  Copy,
  Info,
  Ellipsis,
  Trash2,
  Upload,
  ArrowRight,
  CircleCheck,
} from "lucide-react";
import { useWorkspace } from "../workspace/context";
import { Button, Empty, Field, Modal } from "../shared/ui";
export function ProjectBrowser({
  onConnect,
  onOpen,
}: {
  onConnect: () => void;
  onOpen: () => void;
}) {
  const { language } = useLocale();
  const { controller, projects, connection, api } = useWorkspace();
  const ref = useEntrance<HTMLElement>();
  const [initialPreferences] = useState(readLibraryPreferences);
  const [search, setSearch] = useState(""),
    [layout, setLayout] = useState(initialPreferences.layout),
    [sort, setSort] = useState<SortKey>(initialPreferences.sort),
    [ascending, setAscending] = useState(initialPreferences.ascending),
    [selected, setSelected] = useState<string[]>([]),
    [copying, setCopying] = useState(false),
    [info, setInfo] = useState<ProjectRef | null>(null);
  const [deleting, setDeleting] = useState<ProjectRef[] | null>(null);
  const [restoringPackage, setRestoringPackage] = useState(false);
  const [coverRefresh, setCoverRefresh] = useState<Record<string, number>>({});
  const anchor = useRef<string | null>(null),
    shift = useRef(false);
  const selectAll = useRef<HTMLInputElement>(null);
  useEffect(() => {
    saveLibraryPreferences({ layout, sort, ascending });
  }, [layout, sort, ascending]);

  const filtered = sortProjects(
    projects.filter((project) =>
      project.name.toLocaleLowerCase().includes(search.toLocaleLowerCase()),
    ),
    sort,
    ascending,
  );
  const chosen = projects.filter((project) => selected.includes(project.id));
  const allSelected =
    filtered.length > 0 &&
    filtered.every((project) => selected.includes(project.id));
  useEffect(() => {
    if (selectAll.current)
      selectAll.current.indeterminate =
        !allSelected &&
        filtered.some((project) => selected.includes(project.id));
  }, [allSelected, filtered, selected]);
  useEffect(() => {
    setSelected((ids) =>
      ids.filter((id) => projects.some((project) => project.id === id)),
    );
  }, [projects]);
  const duplicate = async (targets: ProjectRef[]) => {
    setCopying(true);
    try {
      await controller.duplicateProjects(targets, copyNames(projects, targets));
      setSelected([]);
      anchor.current = null;
    } catch (error) {
      controller.report(error);
    } finally {
      setCopying(false);
    }
  };
  const [creating, setCreating] = useState(false),
    [name, setName] = useState(t("Untitled project")),
    [format, setFormat] = useState("landscape"),
    [fps, setFps] = useState(24),
    [pending, setPending] = useState(false);
  const create = async () => {
    setPending(true);
    try {
      const size =
        format === "portrait"
          ? [1080, 1920]
          : format === "square"
            ? [1080, 1080]
            : [1920, 1080];
      await controller.create(name.trim(), size[0], size[1], fps);
      setCreating(false);
      onOpen();
    } catch (error) {
      controller.report(error);
    } finally {
      setPending(false);
    }
  };
  return (
    <main ref={ref} className="project-browser project-home">
      <div className="browser-heading">
        <div>
          <div className="project-home-title">
            <h1>{t("Projects")}</h1>
            <span className="project-count">{projects.length}</span>
          </div>
          <p>{t("Pick up where you left off.")}</p>
        </div>
        <div className="toolbar">
          <Button
            className="project-import"
            icon={<Upload size={17} />}
            disabled={connection !== "connected"}
            onClick={() => setRestoringPackage(true)}
          >
            {t("Import project package")}
          </Button>
          <Button
            className="project-refresh"
            aria-label={t("Refresh projects")}
            disabled={connection !== "connected"}
            icon={<RefreshCw size={16} />}
            onClick={() =>
              void controller
                .refreshProjects()
                .catch((error) => controller.report(error))
            }
          />
          <Button
            className="primary"
            icon={<Plus size={16} />}
            disabled={connection !== "connected"}
            onClick={() => {
              setCreating(true);
            }}
          >
            {t("New project")}
          </Button>
        </div>
      </div>
      <div className="projects-toolbar">
        <div className="search">
          <Search size={15} />
          <input
            aria-label={t("Search projects")}
            placeholder={t("Search projects")}
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </div>
        <div className="toolbar">
          <Select
            aria-label={t("Sort projects")}
            value={sort}
            onChange={(e) => setSort(e.target.value as SortKey)}
          >
            <option value="modified">{t("Recently modified")}</option>
            <option value="created">{t("Created")}</option>
            <option value="name">{t("Name")}</option>
            <option value="duration">{t("Duration")}</option>
          </Select>
          <Button
            aria-label={t(ascending ? "Ascending" : "Descending")}
            onClick={() => setAscending((value) => !value)}
          >
            {ascending ? "↑" : "↓"}
          </Button>
          <Button
            aria-label={t("Grid view")}
            aria-pressed={layout === "grid"}
            icon={<LayoutGrid size={17} />}
            onClick={() => setLayout("grid")}
          />
          <Button
            aria-label={t("List view")}
            aria-pressed={layout === "list"}
            icon={<List size={17} />}
            onClick={() => setLayout("list")}
          />
        </div>
      </div>
      {connection === "connected" &&
        (projects.length > 1 || chosen.length > 0) && (
          <div className="project-selection-toolbar">
            <label>
              <input
                ref={selectAll}
                type="checkbox"
                aria-label={t("Select all projects")}
                checked={allSelected}
                onChange={(e) =>
                  setSelected((ids) =>
                    e.target.checked
                      ? [...new Set([...ids, ...filtered.map((p) => p.id)])]
                      : ids.filter((id) => !filtered.some((p) => p.id === id)),
                  )
                }
              />
              {t("Select all")}
            </label>
            {chosen.length > 0 ? (
              <>
                <span>{t("{count} selected", { count: chosen.length })}</span>
                <Button
                  icon={<Copy size={14} />}
                  disabled={
                    copying ||
                    chosen.length > 64 ||
                    chosen.some(
                      (p) => p.format === "editor" || p.format === "readonly",
                    )
                  }
                  onClick={() => void duplicate(chosen)}
                >
                  {t("Duplicate selected")}
                </Button>
                <Button
                  onClick={() => {
                    setSelected([]);
                    anchor.current = null;
                  }}
                >
                  {t("Clear selection")}
                </Button>
                <Button
                  className="danger"
                  icon={<Trash2 size={14} />}
                  disabled={copying || chosen.length > 64}
                  onClick={() => setDeleting(chosen)}
                >
                  {t("Delete selected")}
                </Button>
                {chosen.length > 64 && (
                  <span className="muted">
                    {t("Select between 1 and 64 projects.")}
                  </span>
                )}
              </>
            ) : (
              <span className="muted">
                {t("{count} projects", { count: projects.length })}
              </span>
            )}
          </div>
        )}
      {connection !== "connected" ? (
        <Empty title={t("Opening your workspace")}>
          <FolderOpen size={36} />
          <span>
            {t("OpenCut is getting ready. If this takes too long, try again.")}
          </span>
          <Button className="primary" onClick={onConnect}>
            {t("Try again")}
          </Button>
        </Empty>
      ) : (
        <div
          className={`project-grid ${layout === "list" ? "project-list" : filtered.length === 1 ? "project-featured" : ""}`}
        >
          {filtered.map((project) => (
            <div
              className={`project-card-wrap ${selected.includes(project.id) ? "selected" : ""}`}
              key={project.id}
            >
              <input
                className="project-select"
                type="checkbox"
                aria-label={t("Select project {name}", { name: project.name })}
                checked={selected.includes(project.id)}
                onClickCapture={(e) => {
                  shift.current = e.shiftKey;
                }}
                onChange={(e) => {
                  const checked = e.target.checked,
                    range = shift.current,
                    previousAnchor = anchor.current;
                  setSelected((ids) =>
                    checked
                      ? range
                        ? selectRange(
                            filtered.map((p) => p.id),
                            ids,
                            previousAnchor,
                            project.id,
                          )
                        : [...new Set([...ids, project.id])]
                      : ids.filter((id) => id !== project.id),
                  );
                  if (
                    checked &&
                    (!range || !filtered.some((p) => p.id === previousAnchor))
                  )
                    anchor.current = project.id;
                  shift.current = false;
                }}
              />
              <button
                className="project-card"
                key={project.id}
                onClick={() =>
                  void controller
                    .open(project)
                    .then(onOpen)
                    .catch((error) => controller.report(error))
                }
              >
                <div className="project-artwork">
                  <ProjectCover
                    api={api}
                    project={project}
                    refresh={coverRefresh[project.id] ?? 0}
                  />
                  {project.library && (
                    <span className="project-duration">
                      {formatTime(project.library.duration)}
                    </span>
                  )}
                </div>
                <div className="project-card-content">
                  <span className="project-card-eyebrow">
                    {t("Recently edited")}
                  </span>
                  <strong>
                    {project.name}
                    {project.format === "readonly" && (
                      <small> · {t("Read-only")}</small>
                    )}
                  </strong>
                  {project.library && (
                    <span className="project-specification">
                      {project.library.width} × {project.library.height}
                      <span aria-hidden="true"> · </span>
                      {Number(
                        (
                          project.library.fps.numerator /
                          project.library.fps.denominator
                        ).toFixed(3),
                      )}{" "}
                      {t("fps")}
                      <span aria-hidden="true"> · </span>
                      {t("{count} media items", {
                        count: project.library.asset_count,
                      })}
                    </span>
                  )}
                  <span className="project-modified">
                    {projectDate(project.library?.modified_at, language)}
                  </span>
                  <span className="project-saved">
                    <CircleCheck size={16} />
                    {t(project.format === "editor" ? "Saved" : "Read-only")}
                  </span>
                  <span className="project-continue">
                    {t(
                      project.format === "editor"
                        ? "Continue editing"
                        : "Open project",
                    )}
                    <ArrowRight size={17} />
                  </span>
                </div>
              </button>
              <details
                className="project-actions"
                onKeyDown={(e) => {
                  if (e.key === "Escape") e.currentTarget.open = false;
                }}
                onBlur={(e) => {
                  if (!e.currentTarget.contains(e.relatedTarget))
                    e.currentTarget.open = false;
                }}
              >
                <summary
                  aria-label={t("Project actions for {name}", {
                    name: project.name,
                  })}
                >
                  <Ellipsis size={17} />
                </summary>
                <div className="project-action-menu">
                  <RenameProject project={project} compact menu />
                  <Button
                    icon={<RefreshCw size={14} />}
                    disabled={
                      project.format === "editor" ||
                      project.format === "readonly"
                    }
                    onClick={(e) => {
                      e.currentTarget
                        .closest("details")
                        ?.removeAttribute("open");
                      api?.covers.invalidate(project.id);
                      setCoverRefresh((values) => ({
                        ...values,
                        [project.id]: (values[project.id] ?? 0) + 1,
                      }));
                    }}
                  >
                    {t("Refresh cover")}
                  </Button>
                  <Button
                    icon={<Copy size={14} />}
                    disabled={
                      copying ||
                      project.format === "editor" ||
                      project.format === "readonly"
                    }
                    onClick={(e) => {
                      e.currentTarget
                        .closest("details")
                        ?.removeAttribute("open");
                      void duplicate([project]);
                    }}
                  >
                    {t("Duplicate project")}
                  </Button>
                  <Button
                    icon={<Info size={14} />}
                    onClick={(e) => {
                      e.currentTarget
                        .closest("details")
                        ?.removeAttribute("open");
                      setInfo(project);
                    }}
                  >
                    {t("Project information")}
                  </Button>
                  <Button
                    className="danger"
                    icon={<Trash2 size={14} />}
                    disabled={copying}
                    onClick={(e) => {
                      e.currentTarget
                        .closest("details")
                        ?.removeAttribute("open");
                      setDeleting([project]);
                    }}
                  >
                    {t("Delete project")}
                  </Button>
                </div>
              </details>
            </div>
          ))}
          {!filtered.length && (
            <Empty
              title={t(
                search ? "No results found" : "Start with a blank canvas",
              )}
            >
              {t(
                search
                  ? "Try a different project name."
                  : "Create your first project, then bring in your media.",
              )}
            </Empty>
          )}
        </div>
      )}
      {connection === "connected" && filtered.length > 0 && (
        <p className="project-results">
          {t("Showing all {count} projects", { count: filtered.length })}
        </p>
      )}
      {restoringPackage && (
        <RestoreProjectPackage
          onClose={() => setRestoringPackage(false)}
          onOpen={onOpen}
        />
      )}
      {info && <ProjectInfo project={info} onClose={() => setInfo(null)} />}
      {deleting && (
        <DeleteProjects projects={deleting} onClose={() => setDeleting(null)} />
      )}
      {creating && (
        <Modal title={t("New project")} onClose={() => setCreating(false)}>
          <form
            onSubmit={(event) => {
              event.preventDefault();
              void create();
            }}
          >
            <Field label={t("Project name")}>
              <input
                required
                autoFocus
                maxLength={200}
                value={name}
                onChange={(event) => setName(event.target.value)}
              />
            </Field>
            <Field label={t("Canvas")}>
              <Select
                value={format}
                onChange={(event) => setFormat(event.target.value)}
              >
                <option value="landscape">
                  {t("Landscape · 1920 × 1080")}
                </option>
                <option value="portrait">{t("Portrait · 1080 × 1920")}</option>
                <option value="square">{t("Square · 1080 × 1080")}</option>
              </Select>
            </Field>
            <Field label={t("Frame rate")}>
              <Select
                value={fps}
                onChange={(event) => setFps(Number(event.target.value))}
              >
                {[24, 25, 30, 50, 60].map((value) => (
                  <option key={value} value={value}>
                    {value} {t("fps")}
                  </option>
                ))}
              </Select>
            </Field>
            <div className="modal-footer">
              <Button onClick={() => setCreating(false)}>{t("Cancel")}</Button>
              <button
                type="submit"
                className="primary"
                disabled={pending || !name.trim()}
              >
                {pending ? t("Creating…") : t("Create project")}
              </button>
            </div>
          </form>
        </Modal>
      )}
    </main>
  );
}
