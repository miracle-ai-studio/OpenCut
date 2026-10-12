import type { ElementSelection } from "../model/selection";
import type { Time } from "../model/types";

export type SortKey = "name" | "created" | "modified" | "duration";
export type LibraryPreferences = {
  layout: "grid" | "list";
  sort: SortKey;
  ascending: boolean;
};
export const defaultLibraryPreferences: LibraryPreferences = {
  layout: "grid",
  sort: "modified",
  ascending: false,
};
export function readLibraryPreferences(): LibraryPreferences {
  try {
    const raw = JSON.parse(localStorage.getItem("opencut.library") ?? "null");
    if (
      raw &&
      ["grid", "list"].includes(raw.layout) &&
      ["name", "created", "modified", "duration"].includes(raw.sort) &&
      typeof raw.ascending === "boolean"
    )
      return { layout: raw.layout, sort: raw.sort, ascending: raw.ascending };
  } catch {
    /* Preferences are optional in private browsing. */
  }
  return defaultLibraryPreferences;
}

export function saveLibraryPreferences(preferences: LibraryPreferences) {
  try {
    localStorage.setItem(
      "opencut.library",
      JSON.stringify({
        layout: preferences.layout,
        sort: preferences.sort,
        ascending: preferences.ascending,
      }),
    );
  } catch {
    /* Preferences are optional. */
  }
}

export type TimelinePreferences = {
  projectId: string;
  sequenceId: string;
  zoom: number;
  scrollLeft: number;
  playhead: Time;
  ripple: boolean;
};

type CoverSceneView = { host: string; projectId: string; sequenceId: string };
// A bounded scene-selection view, never project metadata or cached image bytes.
export class CoverSceneViews {
  private fallback: CoverSceneView[] | null = null;
  constructor(private host: string) {}
  private entries(): CoverSceneView[] {
    if (this.fallback) return this.fallback;
    try {
      const raw: unknown = JSON.parse(
        localStorage.getItem("opencut.cover-scenes.v1") ?? "[]",
      );
      if (!Array.isArray(raw)) return [];
      return raw.slice(-64).flatMap((value) => {
        if (
          !value ||
          typeof value !== "object" ||
          typeof value.host !== "string" ||
          value.host.length > 2048 ||
          typeof value.projectId !== "string" ||
          !value.projectId.length ||
          value.projectId.length > 512 ||
          typeof value.sequenceId !== "string" ||
          !value.sequenceId.length ||
          value.sequenceId.length > 512
        )
          return [];
        let url: URL;
        try {
          url = new URL(value.host);
        } catch {
          return [];
        }
        if (
          !["http:", "https:"].includes(url.protocol) ||
          url.username ||
          url.password ||
          url.search ||
          url.hash
        )
          return [];
        return [
          {
            host: value.host,
            projectId: value.projectId,
            sequenceId: value.sequenceId,
          },
        ];
      });
    } catch {
      return [];
    }
  }
  read(projectId: string): string | null {
    return (
      [...this.entries()]
        .reverse()
        .find((view) => view.host === this.host && view.projectId === projectId)
        ?.sequenceId ?? null
    );
  }
  save(projectId: string, sequenceId: string) {
    if (
      !projectId.length ||
      projectId.length > 512 ||
      !sequenceId.length ||
      sequenceId.length > 512 ||
      this.host.length > 2048
    )
      return;
    try {
      const url = new URL(this.host);
      if (
        !["http:", "https:"].includes(url.protocol) ||
        url.username ||
        url.password ||
        url.search ||
        url.hash
      )
        return;
    } catch {
      return;
    }
    const entries = this.entries().filter(
      (view) => view.host !== this.host || view.projectId !== projectId,
    );
    entries.push({ host: this.host, projectId, sequenceId });
    this.fallback = entries.slice(-64);
    try {
      localStorage.setItem(
        "opencut.cover-scenes.v1",
        JSON.stringify(this.fallback),
      );
      this.fallback = null;
    } catch {
      /* Keep this session's view if browser storage is unavailable. */
    }
  }
}
const pending = new Map<string, TimelinePreferences>();
let timer: ReturnType<typeof setTimeout> | undefined;
const scopeKey = (projectId: string, sequenceId: string) =>
  JSON.stringify([projectId, sequenceId]);
const validTime = (value: unknown): value is Time => {
  if (!value || typeof value !== "object") return false;
  const { ticks, timebase } = value as Time;
  return (
    typeof ticks === "string" &&
    /^(0|[1-9]\d{0,18})$/.test(ticks) &&
    BigInt(ticks) <= 2n ** 63n - 1n &&
    Number.isInteger(timebase) &&
    timebase > 0 &&
    timebase <= 4294967295
  );
};
function timelineEntries(): TimelinePreferences[] {
  try {
    const raw: unknown = JSON.parse(
      localStorage.getItem("opencut.timeline") ?? "[]",
    );
    if (!Array.isArray(raw)) return [];
    return raw.slice(-64).flatMap((item) => {
      if (
        !item ||
        typeof item.projectId !== "string" ||
        typeof item.sequenceId !== "string" ||
        item.projectId.length > 512 ||
        item.sequenceId.length > 512 ||
        !Number.isFinite(item.zoom) ||
        item.zoom < 0.1 ||
        item.zoom > 8 ||
        !Number.isFinite(item.scrollLeft) ||
        item.scrollLeft < 0 ||
        item.scrollLeft > 1e7 ||
        !validTime(item.playhead)
      )
        return [];
      return [
        {
          projectId: item.projectId,
          sequenceId: item.sequenceId,
          zoom: item.zoom,
          scrollLeft: item.scrollLeft,
          playhead: {
            ticks: item.playhead.ticks,
            timebase: item.playhead.timebase,
          },
          ripple: item.ripple === true,
        },
      ];
    });
  } catch {
    return [];
  }
}
export function readTimelinePreferences(
  projectId: string,
  sequenceId: string,
): TimelinePreferences {
  return (
    pending.get(scopeKey(projectId, sequenceId)) ??
    timelineEntries()
      .reverse()
      .find(
        (entry) =>
          entry.projectId === projectId && entry.sequenceId === sequenceId,
      ) ?? {
      projectId,
      sequenceId,
      zoom: 1,
      scrollLeft: 0,
      playhead: { ticks: "0", timebase: 1 },
      ripple: false,
    }
  );
}
export function saveTimelinePreferences(
  projectId: string,
  sequenceId: string,
  patch: Partial<
    Pick<TimelinePreferences, "zoom" | "scrollLeft" | "playhead" | "ripple">
  >,
) {
  const before = readTimelinePreferences(projectId, sequenceId);
  const next = {
    projectId,
    sequenceId,
    zoom: patch.zoom ?? before.zoom,
    scrollLeft: patch.scrollLeft ?? before.scrollLeft,
    playhead: patch.playhead ?? before.playhead,
    ripple: patch.ripple ?? before.ripple,
  };
  if (
    projectId.length > 512 ||
    sequenceId.length > 512 ||
    !Number.isFinite(next.zoom) ||
    next.zoom < 0.1 ||
    next.zoom > 8 ||
    !Number.isFinite(next.scrollLeft) ||
    next.scrollLeft < 0 ||
    next.scrollLeft > 1e7 ||
    !validTime(next.playhead)
  )
    return;
  pending.set(scopeKey(projectId, sequenceId), next);
  if (timer === undefined) timer = setTimeout(flushTimelinePreferences, 200);
}
export function flushTimelinePreferences() {
  clearTimeout(timer);
  timer = undefined;
  if (!pending.size) return;
  const entries = new Map(
    timelineEntries().map((entry) => [
      scopeKey(entry.projectId, entry.sequenceId),
      entry,
    ]),
  );
  for (const [key, entry] of pending) {
    entries.delete(key);
    entries.set(key, entry);
  }
  try {
    localStorage.setItem(
      "opencut.timeline",
      JSON.stringify([...entries.values()].slice(-64)),
    );
  } catch {
    /* View preferences remain optional. */
  }
  pending.clear();
}
if (typeof window !== "undefined")
  window.addEventListener("pagehide", flushTimelinePreferences);

export type WorkspaceLayouts = {
  stage?: Record<"library" | "preview" | "properties", number>;
  workspace?: Record<"stage" | "timeline", number>;
};
function panelLayout(
  value: unknown,
  ids: readonly string[],
): Record<string, number> | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) return;
  const raw = value as Record<string, unknown>,
    projected: Record<string, number> = {};
  let sum = 0;
  for (const id of ids) {
    const size = raw[id];
    if (
      typeof size !== "number" ||
      !Number.isFinite(size) ||
      size < 5 ||
      size > 95
    )
      return;
    projected[id] = size;
    sum += size;
  }
  return Math.abs(sum - 100) < 0.1 ? projected : undefined;
}
export function readWorkspaceLayouts(): WorkspaceLayouts {
  try {
    const raw = JSON.parse(localStorage.getItem("opencut.layout") ?? "null");
    if (raw && typeof raw === "object")
      return {
        stage: panelLayout(raw.stage, [
          "library",
          "preview",
          "properties",
        ]) as WorkspaceLayouts["stage"],
        workspace: panelLayout(raw.workspace, [
          "stage",
          "timeline",
        ]) as WorkspaceLayouts["workspace"],
      };
  } catch {}
  return {};
}
export function saveWorkspaceLayout(
  group: keyof WorkspaceLayouts,
  value: Record<string, number>,
) {
  if (group !== "stage" && group !== "workspace") return;
  const layout = panelLayout(
    value,
    group === "stage"
      ? ["library", "preview", "properties"]
      : ["stage", "timeline"],
  );
  if (!layout) return;
  try {
    localStorage.setItem(
      "opencut.layout",
      JSON.stringify({ ...readWorkspaceLayouts(), [group]: layout }),
    );
  } catch {}
}
export type WelcomePreferences = {
  completed: boolean;
  lastSeenRelease: string | null;
};
export function readWelcomePreferences(): WelcomePreferences {
  try {
    const raw = JSON.parse(localStorage.getItem("opencut.welcome") ?? "null");
    if (raw && typeof raw === "object")
      return {
        completed: raw.completed === true,
        lastSeenRelease:
          typeof raw.lastSeenRelease === "string" &&
          /^\d{1,8}\.\d{1,8}\.\d{1,8}$/.test(raw.lastSeenRelease)
            ? raw.lastSeenRelease
            : null,
      };
  } catch {}
  return { completed: false, lastSeenRelease: null };
}
export function saveWelcomePreferences(next: Partial<WelcomePreferences>) {
  const current = readWelcomePreferences(),
    completed =
      typeof next.completed === "boolean" ? next.completed : current.completed,
    lastSeenRelease =
      next.lastSeenRelease &&
      /^\d{1,8}\.\d{1,8}\.\d{1,8}$/.test(next.lastSeenRelease)
        ? next.lastSeenRelease
        : current.lastSeenRelease;
  try {
    localStorage.setItem(
      "opencut.welcome",
      JSON.stringify({ completed, lastSeenRelease }),
    );
  } catch {}
}
export function unseenUpstreamRelease(version: string, seen: string | null) {
  if (!seen) return true;
  const a = version.split(".").map(BigInt),
    b = seen.split(".").map(BigInt);
  for (let i = 0; i < 3; i++) {
    if (a[i] !== b[i]) return a[i] > b[i];
  }
  return false;
}

export type PreviewGuides = {
  kind: "none" | "grid" | "tiktok" | "custom";
  rows: number;
  cols: number;
  lines: { axis: "x" | "y"; position: number }[];
};
export const defaultPreviewGuides: PreviewGuides = {
  kind: "none",
  rows: 3,
  cols: 3,
  lines: [],
};
function previewGuides(value: unknown): PreviewGuides | null {
  if (!value || typeof value !== "object") return null;
  const raw = value as PreviewGuides;
  if (
    !["none", "grid", "tiktok", "custom"].includes(raw.kind) ||
    !Number.isInteger(raw.rows) ||
    raw.rows < 1 ||
    raw.rows > 24 ||
    !Number.isInteger(raw.cols) ||
    raw.cols < 1 ||
    raw.cols > 24 ||
    !Array.isArray(raw.lines) ||
    raw.lines.length > 32 ||
    raw.lines.some(
      (line) =>
        !line ||
        !["x", "y"].includes(line.axis) ||
        !Number.isFinite(line.position) ||
        line.position < 0 ||
        line.position > 100,
    )
  )
    return null;
  return {
    kind: raw.kind,
    rows: raw.rows,
    cols: raw.cols,
    lines: raw.lines.map(({ axis, position }) => ({ axis, position })),
  };
}
export function readPreviewGuides(): PreviewGuides {
  try {
    return (
      previewGuides(
        JSON.parse(localStorage.getItem("opencut.preview") ?? "null"),
      ) ?? defaultPreviewGuides
    );
  } catch {
    return defaultPreviewGuides;
  }
}
export function savePreviewGuides(value: PreviewGuides) {
  const projected = previewGuides(value);
  if (!projected) return;
  try {
    localStorage.setItem("opencut.preview", JSON.stringify(projected));
  } catch {}
}

// Optional client view hints, never a source of project/history authority.
export type HistoryView = {
  sequenceId: string | null;
  selections: ElementSelection[];
  playhead: Time;
};
export type HistoryViews = {
  host: string;
  projectId: string;
  transactionId: string;
  before: HistoryView;
  after: HistoryView;
};
const limit = 128;
const id = (value: unknown): value is string =>
  typeof value === "string" && value.length > 0 && value.length <= 512;
function view(value: unknown): HistoryView | null {
  if (!value || typeof value !== "object") return null;
  const raw = value as HistoryView;
  if (
    (raw.sequenceId !== null && !id(raw.sequenceId)) ||
    !Array.isArray(raw.selections) ||
    raw.selections.length > 128 ||
    raw.selections.some(
      (item) =>
        !item ||
        !["clip", "caption", "shape"].includes(item.kind) ||
        !id(item.id) ||
        (item.kind === "clip" && !id(item.trackId)),
    ) ||
    !raw.playhead ||
    typeof raw.playhead.ticks !== "string" ||
    !/^(0|[1-9]\d{0,18})$/.test(raw.playhead.ticks) ||
    BigInt(raw.playhead.ticks) > 2n ** 63n - 1n ||
    !Number.isInteger(raw.playhead.timebase) ||
    raw.playhead.timebase <= 0 ||
    raw.playhead.timebase > 4294967295
  )
    return null;
  return {
    sequenceId: raw.sequenceId,
    selections: raw.selections.map((item) =>
      item.kind === "clip"
        ? { kind: item.kind, id: item.id, trackId: item.trackId }
        : { kind: item.kind, id: item.id },
    ),
    playhead: { ticks: raw.playhead.ticks, timebase: raw.playhead.timebase },
  };
}
export class HistoryViewStore {
  private entries: HistoryViews[] = [];
  private load(): HistoryViews[] {
    try {
      const text = localStorage.getItem("opencut.history-views.v1");
      if (!text) return [];
      if (text.length > 2_000_000) return [];
      const raw: unknown = JSON.parse(text);
      if (!Array.isArray(raw)) return [];
      return raw.slice(-limit).flatMap((item) => {
        if (
          !item ||
          !id(item.host) ||
          !id(item.projectId) ||
          !id(item.transactionId)
        )
          return [];
        const before = view(item.before),
          after = view(item.after);
        return before && after
          ? [
              {
                host: item.host,
                projectId: item.projectId,
                transactionId: item.transactionId,
                before,
                after,
              },
            ]
          : [];
      });
    } catch {
      return [];
    }
  }
  read(host: string, projectId: string, transactionId: string) {
    return structuredClone(
      [...this.load(), ...this.entries]
        .reverse()
        .find(
          (item) =>
            item.host === host &&
            item.projectId === projectId &&
            item.transactionId === transactionId,
        ) ?? null,
    );
  }
  save(entry: HistoryViews) {
    const before = view(entry.before),
      after = view(entry.after);
    if (
      !id(entry.host) ||
      !id(entry.projectId) ||
      !id(entry.transactionId) ||
      !before ||
      !after
    )
      return;
    const next = {
      host: entry.host,
      projectId: entry.projectId,
      transactionId: entry.transactionId,
      before,
      after,
    };
    const matches = (item: HistoryViews) =>
      item.host === next.host &&
      item.projectId === next.projectId &&
      item.transactionId === next.transactionId;
    this.entries = [
      ...this.entries.filter((item) => !matches(item)),
      next,
    ].slice(-limit);
    try {
      const merged = [
        ...this.load().filter((item) => !matches(item)),
        next,
      ].slice(-limit);
      localStorage.setItem("opencut.history-views.v1", JSON.stringify(merged));
    } catch {
      // Private browsing/storage denial keeps the current session usable.
    }
  }
}
