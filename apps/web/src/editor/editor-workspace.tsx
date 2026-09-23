import { useEffect, useMemo, useRef, useState } from "react";
import {
  Clapperboard,
  CloudUpload,
  Film,
  FolderOpen,
  Plus,
  RefreshCw,
  Scissors,
  Trash2,
  Wifi,
  WifiOff,
} from "lucide-react";
import {
  EditorClient,
  addTime,
  compareTime,
  halfTime,
  scaleTime,
  secondsForDisplay,
  subtractWholeSeconds,
  timeFromDecimalSeconds,
  type Capabilities,
  type Asset,
  type CaptionStyle,
  type Job,
  type JobEvent,
  type Operation,
  type ParameterDescriptor,
  type RenderPreflightReport,
  type RevisionPage,
  type Snapshot,
  type Time,
} from "../lib/editor-client";
import "./editor.css";
import "./panels.css";
import { isCurrentProjectFrameRequest } from "./project-frame";
import {
  isCurrentWorkspace,
  isSameWorkspaceVisit,
  loadCurrentSourcePreview,
  prepareCurrentPreviewRequest,
  readCurrentProjectState,
  type WorkspaceIdentity,
} from "./workspace-context";
import {
  assetReplaceOperation,
  assetVersionView,
  trackKindForAsset,
} from "./media-assets";
import { isCurrentRenderRevision, mediaAssetLock } from "./render-preflight";

const t = (ticks: number, timebase = 24) => ({
  ticks: String(ticks),
  timebase,
});
const n = secondsForDisplay;
const isProjectFrameJob = (job: Job) =>
  job.kind === "preview.project_frame" || job.kind === "preview.composited_frame";
const resultTime = (value: unknown): Time | null => {
  if (!value || typeof value !== "object") return null;
  const candidate = value as { ticks?: unknown; timebase?: unknown };
  return typeof candidate.ticks === "string" &&
    /^-?\d+$/.test(candidate.ticks) &&
    typeof candidate.timebase === "number" &&
    Number.isInteger(candidate.timebase) && candidate.timebase > 0
    ? { ticks: candidate.ticks, timebase: candidate.timebase }
    : null;
};
const rangePreviewLabel = (job: Job) => {
  const result = job.result as {
    revision?: string;
    actual_sequence_range?: { start?: unknown; duration?: unknown };
  } | null;
  const start = resultTime(result?.actual_sequence_range?.start);
  const duration = resultTime(result?.actual_sequence_range?.duration);
  const range = start && duration
    ? `${n(start).toFixed(3)}–${n(addTime(start, duration)).toFixed(3)}s`
    : "UNKNOWN RANGE";
  return `PROJECT RANGE · REV ${(result?.revision ?? "UNKNOWN").slice(0, 12)} · ${range} · PLAYER STARTS AT 0s`;
};
const jobErrorText = (value: unknown) => {
  if (!value || typeof value !== "object") return null;
  const error = value as { code?: unknown; message?: unknown };
  const code = typeof error.code === "string" ? error.code : null,
    message = typeof error.message === "string" ? error.message : null;
  return [code, message].filter(Boolean).join(": ") || null;
};
const progressOf = (job: Job) => job.progress ?? null;
export function EditorWorkspace() {
  const [url, setUrl] = useState("http://127.0.0.1:8788"),
    [token, setToken] = useState(""),
    [client, setClient] = useState<EditorClient | null>(null),
    [projects, setProjects] = useState<Snapshot[]>([]),
    [snap, setSnap] = useState<Snapshot | null>(null),
    [selectedIds, setSelectedIds] = useState<{
      trackId: string;
      clipId: string;
    } | null>(null),
    [status, setStatus] = useState("Disconnected"),
    [busy, setBusy] = useState(false),
    [transitioning, setTransitioning] = useState(false),
    [log, setLog] = useState<Array<{ at: string; label: string; ok: boolean }>>(
      [],
    ),
    [raw, setRaw] = useState("[]"),
    [preview, setPreview] = useState<string | null>(null),
    [previewType, setPreviewType] = useState<"video" | "image">("video"),
    [previewLabel, setPreviewLabel] = useState("SOURCE MONITOR"),
    [jobs, setJobs] = useState<Job[]>([]),
    [jobEvents, setJobEvents] = useState<JobEvent[]>([]),
    [captionText, setCaptionText] = useState(""),
    [captionStart, setCaptionStart] = useState("0"),
    [captionDuration, setCaptionDuration] = useState("3"),
    [captionFontFamily, setCaptionFontFamily] = useState("Arial"),
    [captionFontSize, setCaptionFontSize] = useState("48"),
    [captionFallback, setCaptionFallback] = useState(""),
    [captionAlign, setCaptionAlign] = useState<"left" | "center" | "right">(
      "center",
    ),
    [captionMaxWidth, setCaptionMaxWidth] = useState(""),
    [captionLineHeight, setCaptionLineHeight] = useState(""),
    [frameTime, setFrameTime] = useState("0"),
    [rangeDuration, setRangeDuration] = useState("3"),
    [frameCreating, setFrameCreating] = useState(false),
    [frameRequest, setFrameRequest] = useState<{
      jobId: string;
      projectId: string;
      revision: string;
      generation: number;
      requestedTime: string;
    } | null>(null),
    [keyframeParameter, setKeyframeParameter] = useState("opacity"),
    [keyframeTime, setKeyframeTime] = useState("0"),
    [keyframeValue, setKeyframeValue] = useState("1"),
    [keyframeInterpolation, setKeyframeInterpolation] = useState<
      "hold" | "linear"
    >("linear"),
    [history, setHistory] = useState<RevisionPage | null>(null),
    [historical, setHistorical] = useState<Snapshot | null>(null),
    [redoRevision, setRedoRevision] = useState<string | null>(null),
    [parameters, setParameters] = useState<ParameterDescriptor[]>([]),
    [capabilities, setCapabilities] = useState<Capabilities | null>(null);
  const [preflight, setPreflight] = useState<RenderPreflightReport | null>(
      null,
    ),
    [preflightChecking, setPreflightChecking] = useState(false);
  const jobCursor = useRef(0),
    streamGeneration = useRef(0),
    frameGeneration = useRef(0),
    frameArtifactDownload = useRef<{
      jobId: string;
      controller: AbortController;
    } | null>(null),
    preflightGeneration = useRef(0),
    preflightAbort = useRef<AbortController | null>(null),
    busyGeneration = useRef(0),
    workspaceGeneration = useRef(0),
    navigationPending = useRef(false),
    writeInFlight = useRef(false),
    headContext = useRef<WorkspaceIdentity<EditorClient> | null>(null),
    sourceGeneration = useRef(0),
    workspaceContext = useRef<WorkspaceIdentity<EditorClient> | null>(null),
    sourceContext = useRef<WorkspaceIdentity<EditorClient> | null>(null),
    frameContext = useRef<{
      api: EditorClient | null;
      projectId: string | null;
      revision: string | null;
      sequenceId: string | null;
      historicalRevision: string | null;
    }>({
      api: null,
      projectId: null,
      revision: null,
      sequenceId: null,
      historicalRevision: null,
    });
  const api = client;
  const shown = historical ?? snap;
  headContext.current =
    api && snap && !navigationPending.current
      ? {
          connection: api,
          projectId: snap.project.id,
          revision: snap.revision,
          generation: workspaceGeneration.current,
        }
      : null;
  workspaceContext.current = historical ? null : headContext.current;
  sourceContext.current =
    api && snap && !historical && !navigationPending.current
      ? {
          connection: api,
          projectId: snap.project.id,
          revision: snap.revision,
          generation: sourceGeneration.current,
        }
      : null;
  frameContext.current = {
    api: navigationPending.current ? null : api,
    projectId: snap?.project.id ?? null,
    revision: snap?.revision ?? null,
    sequenceId: snap?.project.default_sequence_id ?? null,
    historicalRevision: historical?.revision ?? null,
  };
  const assets = shown?.project.assets ?? [];
  const seq = shown?.project.sequences.find(
    (x) => x.id === shown.project.default_sequence_id,
  );
  const selected = useMemo(() => {
    if (!seq || !selectedIds) return null;
    const track = seq.tracks.find((item) => item.id === selectedIds.trackId);
    const clip = track?.clips.find((item) => item.id === selectedIds.clipId);
    return track && clip ? { track, clip } : null;
  }, [seq, selectedIds]);
  const framePreviewAvailable =
    capabilities?.rendering.project_frame_preview === true;
  const compositedFrameAvailable =
    capabilities?.rendering.composited_frame_preview === true;
  const rangePreviewAvailable =
    capabilities?.rendering.project_range_preview === true;
  const currentSourceWorkspace = () => sourceContext.current;
  // Invalidate synchronously, before React renders or any network request starts.
  // An unrelated render must not resurrect the previous writable workspace.
  const beginNavigation = () => {
    const generation = ++workspaceGeneration.current;
    navigationPending.current = true;
    setTransitioning(true);
    headContext.current = null;
    workspaceContext.current = null;
    sourceGeneration.current++;
    sourceContext.current = null;
    frameGeneration.current++;
    frameContext.current.api = null;
    frameArtifactDownload.current?.controller.abort();
    frameArtifactDownload.current = null;
    preflightGeneration.current++;
    preflightAbort.current?.abort();
    preflightAbort.current = null;
    setPreflight(null);
    setPreflightChecking(false);
    setFrameRequest(null);
    setFrameCreating(false);
    setPreview(null);
    setPreviewLabel("SOURCE MONITOR");
    setSelectedIds(null);
    return generation;
  };
  const finishNavigation = (generation: number) => {
    if (generation !== workspaceGeneration.current) return false;
    navigationPending.current = false;
    setTransitioning(false);
    return true;
  };
  const beginSourceRequest = () => {
    if (!api || !snap || historical || navigationPending.current) return null;
    frameGeneration.current++;
    frameArtifactDownload.current?.controller.abort();
    frameArtifactDownload.current = null;
    setFrameRequest(null);
    setFrameCreating(false);
    const request = {
      connection: api,
      projectId: snap.project.id,
      revision: snap.revision,
      generation: ++sourceGeneration.current,
    };
    sourceContext.current = request;
    return request;
  };
  const act = async (
    label: string,
    fn: () => Promise<unknown>,
    allowHistorical = false,
  ): Promise<boolean> => {
    const request = allowHistorical ? headContext.current : workspaceContext.current;
    if (!request || writeInFlight.current) return false;
    writeInFlight.current = true;
    const busyRequest = ++busyGeneration.current;
    setBusy(true);
    try {
      await fn();
      if (!isCurrentWorkspace(request, headContext.current)) return false;
      // Any successful write starts a new branch of local undo/redo history.
      setRedoRevision(null);
      setLog((x) => [
        { at: new Date().toLocaleTimeString(), label, ok: true },
        ...x,
      ]);
      if (request) {
        const state = await readCurrentProjectState({
          request,
          current: () => headContext.current,
          readSnapshot: () => request.connection.snapshot(request.projectId),
          readHistory: () => request.connection.history(request.projectId),
        });
        if (state) {
          setSnap(state.snapshot);
          setHistory(state.history);
          setStatus("Saved");
          return true;
        }
      }
    } catch (e) {
      const err = e as Error & { code?: string };
      setLog((x) => [
        {
          at: new Date().toLocaleTimeString(),
          label: `${label}: ${err.message}`,
          ok: false,
        },
        ...x,
      ]);
      if (isCurrentWorkspace(request, headContext.current)) {
        setStatus(
          err.code === "REVISION_CONFLICT"
            ? "Revision changed — refreshed"
            : "Error",
        );
        if (err.code === "REVISION_CONFLICT") {
          setRedoRevision(null);
          try {
            const state = await readCurrentProjectState({
              request,
              current: () => headContext.current,
              readSnapshot: () => request.connection.snapshot(request.projectId),
              readHistory: () => request.connection.history(request.projectId),
            });
            if (state) {
              setSnap(state.snapshot);
              setHistory(state.history);
            }
          } catch (refreshError) {
            if (isCurrentWorkspace(request, headContext.current))
              setStatus(`Conflict; refresh failed: ${(refreshError as Error).message}`);
          }
        }
      }
    } finally {
      writeInFlight.current = false;
      if (busyGeneration.current === busyRequest) setBusy(false);
    }
    return false;
  };
  const connect = async () => {
    const generation = beginNavigation();
    streamGeneration.current++;
    setClient(null);
    setSnap(null);
    setProjects([]);
    setHistory(null);
    setHistorical(null);
    setRedoRevision(null);
    setCapabilities(null);
    setParameters([]);
    setJobs([]);
    setJobEvents([]);
    setStatus("Connecting");
    const busyRequest = ++busyGeneration.current;
    setBusy(true);
    try {
      const c = new EditorClient(url, token);
      const [nextCapabilities, list] = await Promise.all([c.capabilities(), c.projects()]);
      const [snapshot, nextHistory] = list[0]
        ? await Promise.all([c.snapshot(list[0].project.id), c.history(list[0].project.id)])
        : [null, null];
      if (!finishNavigation(generation)) return;
      setCapabilities(nextCapabilities);
      setParameters(nextCapabilities.parameters ?? []);
      setClient(c);
      setProjects(list);
      setSnap(snapshot);
      setHistory(nextHistory);
      setStatus("Connected");
    } catch (e) {
      if (finishNavigation(generation)) setStatus(`Disconnected: ${(e as Error).message}`);
    } finally {
      if (busyGeneration.current === busyRequest) setBusy(false);
    }
  };
  const apply = (label: string, ops: Operation[]) =>
    historical
      ? (setStatus("Historical snapshots are read-only"), Promise.resolve())
      : api && snap
        ? act(label, () =>
            api.apply(snap.project.id, snap.revision, ops, label),
          )
        : Promise.resolve();
  const choose = async (id: string) => {
    if (!api) return;
    const connection = api, generation = beginNavigation();
    setSnap(null);
    setHistory(null);
    setHistorical(null);
    setRedoRevision(null);
    try {
      if (connection) {
        const [snapshot, nextHistory] = await Promise.all([
          connection.snapshot(id),
          connection.history(id),
        ]);
        if (!finishNavigation(generation)) return;
        setSnap(snapshot);
        setHistory(nextHistory);
      }
      setHistorical(null);
      setRedoRevision(null);
      setSelectedIds(null);
    } catch (error) {
      if (finishNavigation(generation))
        setStatus((error as Error).message);
    }
  };
  const createProject = async () => {
    if (!api) return;
    const connection = api, generation = beginNavigation();
    setSnap(null);
    setHistory(null);
    setHistorical(null);
    setRedoRevision(null);
    const busyRequest = ++busyGeneration.current;
    setBusy(true);
    try {
      const id = crypto.randomUUID(),
        s = await connection.create({
          id,
          name: `Untitled ${projects.length + 1}`,
          sequence_id: crypto.randomUUID(),
          width: 1920,
          height: 1080,
          fps: { numerator: 24, denominator: 1 },
        });
      const [list, nextHistory] = await Promise.all([
        connection.projects(), connection.history(s.project.id),
      ]);
      if (!finishNavigation(generation)) return;
      setProjects(list);
      setSnap(s);
      setHistory(nextHistory);
      setStatus("Saved");
      setLog((x) => [
        {
          at: new Date().toLocaleTimeString(),
          label: "Create project",
          ok: true,
        },
        ...x,
      ]);
    } catch (e) {
      if (finishNavigation(generation)) setStatus((e as Error).message);
    } finally {
      if (busyGeneration.current === busyRequest) setBusy(false);
    }
  };
  const addTrack = () =>
    seq &&
    apply("Create video track", [
      {
        type: "track.create",
        sequence_id: seq.id,
        track: {
          id: crypto.randomUUID(),
          name: `Video ${seq.tracks.length + 1}`,
          kind: "video",
          enabled: true,
          muted: false,
          locked: false,
          clips: [],
          params: {},
        },
      },
    ]);
  const split = () => {
    if (!selected || !seq) return;
    const r = selected.clip.timeline_range,
      at = addTime(r.start, halfTime(r.duration));
    apply("Split clip", [
      {
        type: "clip.split",
        sequence_id: seq.id,
        track_id: selected.track.id,
        clip_id: selected.clip.id,
        at,
        right_clip_id: crypto.randomUUID(),
      },
    ]);
  };
  const trim = () => {
    if (!selected || !seq) return;
    const c = selected.clip,
      newDuration = subtractWholeSeconds(c.timeline_range.duration, 1n);
    if (BigInt(newDuration.ticks) <= 0n) {
      setStatus("Clip is too short to trim");
      return;
    }
    const source_range = c.source_range
      ? {
          ...c.source_range,
          duration: scaleTime(
            c.source_range.duration,
            newDuration,
            c.timeline_range.duration,
          ),
        }
      : null;
    apply("Trim one second", [
      {
        type: "clip.trim",
        sequence_id: seq.id,
        track_id: selected.track.id,
        clip_id: c.id,
        timeline_range: { ...c.timeline_range, duration: newDuration },
        source_range,
      },
    ]);
  };
  const del = () =>
    selected &&
    seq &&
    apply("Delete clip", [
      {
        type: "clip.delete",
        sequence_id: seq.id,
        track_id: selected.track.id,
        clip_id: selected.clip.id,
      },
    ]).then((saved) => { if (saved) setSelectedIds(null); });
  const importMedia = async (file: File) => {
    if (!api || !snap || !seq) return;
    const editRequest = workspaceContext.current;
    if (!editRequest) return;
    const sourceRequest = beginSourceRequest();
    if (!sourceRequest) return;
    let previewWarning: string | null = null;
    await act(`Import ${file.name}`, async () => {
      const up = await api.upload(file);
      if (!isCurrentWorkspace(editRequest, workspaceContext.current))
        throw new Error("Import was not applied because the workspace changed");
      const trackKind = trackKindForAsset(up.asset.kind);
      let track = seq.tracks.find((x) => x.kind === trackKind);
      const ops: Operation[] = assets.some((asset) => asset.id === up.asset.id)
        ? []
        : [{ type: "asset.bind", asset: up.asset }];
      if (!track) {
        track = {
          id: crypto.randomUUID(),
          name:
            trackKind === "audio"
              ? "Audio 1"
              : trackKind === "graphic"
                ? "Graphic 1"
                : "Video 1",
          kind: trackKind,
          enabled: true,
          muted: false,
          locked: false,
          clips: [],
          params: {},
        };
        ops.push({ type: "track.create", sequence_id: seq.id, track });
      }
      const duration = up.asset.duration ?? t(120);
      let start = t(0);
      for (const existing of track.clips) {
        const end = addTime(
          existing.timeline_range.start,
          existing.timeline_range.duration,
        );
        if (compareTime(end, start) > 0) start = end;
      }
      ops.push({
        type: "clip.insert",
        sequence_id: seq.id,
        track_id: track.id,
        clip: {
          id: crypto.randomUUID(),
          asset_id: up.asset.id,
          timeline_range: { start, duration },
          source_range:
            up.asset.kind === "image" ? null : { start: t(0), duration },
          params: {},
          channels: [],
        },
      });
      await api.apply(
        snap.project.id,
        snap.revision,
        ops,
        `Import ${file.name}`,
      );
      const source = await loadCurrentSourcePreview({
        request: sourceRequest,
        current: currentSourceWorkspace,
        load: () => api.mediaBlob(up.media_url),
        revoke: URL.revokeObjectURL,
      });
      if (source.url) {
        setPreview(source.url);
        setPreviewType(up.asset.kind === "image" ? "image" : "video");
        setPreviewLabel("SOURCE MONITOR");
      } else if (source.error) {
        previewWarning = source.error.message;
      }
    });
    if (
      previewWarning &&
      isSameWorkspaceVisit(sourceRequest, currentSourceWorkspace())
    )
      setStatus(`Saved · source preview unavailable: ${previewWarning}`);
  };
  const replaceMedia = async (asset: Asset, file: File) => {
    if (!api || !snap || historical) return;
    const editRequest = workspaceContext.current;
    if (!editRequest) return;
    const sourceRequest = beginSourceRequest();
    if (!sourceRequest) return;
    let previewWarning: string | null = null;
    const projectId = snap.project.id,
      revision = snap.revision;
    await act(`Replace media for ${asset.name}`, async () => {
      const uploaded = await api.upload(file),
        operation = assetReplaceOperation(asset, uploaded.asset);
      if (!isCurrentWorkspace(editRequest, workspaceContext.current))
        throw new Error("Replacement was not applied because the workspace changed");
      await api.apply(
        projectId,
        revision,
        [operation],
        `Replace ${asset.name}`,
      );
      const source = await loadCurrentSourcePreview({
        request: sourceRequest,
        current: currentSourceWorkspace,
        load: () => api.mediaBlob(uploaded.media_url),
        revoke: URL.revokeObjectURL,
      });
      if (source.url) {
        setPreview(source.url);
        setPreviewType(uploaded.asset.kind === "image" ? "image" : "video");
        setPreviewLabel("SOURCE MONITOR · UPLOADED REPLACEMENT");
      } else if (source.error) {
        previewWarning = source.error.message;
      }
    });
    if (
      previewWarning &&
      isSameWorkspaceVisit(sourceRequest, currentSourceWorkspace())
    )
      setStatus(`Saved · source preview unavailable: ${previewWarning}`);
  };
  useEffect(
    () => () => {
      if (preview) URL.revokeObjectURL(preview);
    },
    [preview],
  );
  useEffect(() => {
    sourceGeneration.current++;
    sourceContext.current = null;
    frameGeneration.current++;
    frameArtifactDownload.current?.controller.abort();
    frameArtifactDownload.current = null;
    setFrameCreating(false);
    setFrameRequest(null);
    preflightGeneration.current++;
    preflightAbort.current?.abort();
    preflightAbort.current = null;
    setPreflightChecking(false);
    setPreflight(null);
  }, [api, snap?.project.id, snap?.revision, historical?.revision]);
  const duration = useMemo(
    () =>
      Math.max(
        10,
        ...(seq?.tracks
          .flatMap((x) => x.clips)
          .map(
            (c) => n(c.timeline_range.start) + n(c.timeline_range.duration),
          ) ?? []),
      ),
    [seq],
  );
  useEffect(() => {
    if (seq)
      setRaw(
        JSON.stringify(
          [
            {
              type: "track.create",
              sequence_id: seq.id,
              track: {
                id: crypto.randomUUID(),
                name: `Video ${seq.tracks.length + 1}`,
                kind: "video",
                enabled: true,
                muted: false,
                locked: false,
                clips: [],
                params: {},
              },
            },
          ],
          null,
          2,
        ),
      );
  }, [seq?.id]);
  useEffect(() => {
    if (!api) return;
    const controller = new AbortController(),
      generation = ++streamGeneration.current;
    let active = true,
      jobsRequest = 0;
    jobCursor.current = 0;
    setJobEvents([]);
    const refreshJobs = () => {
      const request = ++jobsRequest;
      return api
        .jobs()
        .then((value) => {
          if (
            active &&
            generation === streamGeneration.current &&
            request === jobsRequest
          )
            setJobs(value);
        })
        .catch((error) => {
          if (
            active &&
            generation === streamGeneration.current &&
            request === jobsRequest
          )
            setStatus(`Jobs: ${(error as Error).message}`);
        });
    };
    void refreshJobs();
    const run = async () => {
      while (active && !controller.signal.aborted) {
        try {
          const nextCursor = await api.streamJobEvents(
            jobCursor.current,
            null,
            controller.signal,
            (raw, sequence) => {
              if (!active || generation !== streamGeneration.current) return;
              jobCursor.current = Math.max(jobCursor.current, sequence);
              const event = raw;
              setJobEvents((current) =>
                [
                  event,
                  ...current.filter((item) => item.sequence !== event.sequence),
                ].slice(0, 100),
              );
              void refreshJobs();
            },
          );
          if (active && generation === streamGeneration.current)
            jobCursor.current = Math.max(jobCursor.current, nextCursor);
        } catch (error) {
          if (controller.signal.aborted) return;
          if (active && generation === streamGeneration.current)
            setStatus(`Job stream reconnecting: ${(error as Error).message}`);
        }
        await new Promise((resolve) => window.setTimeout(resolve, 1000));
      }
    };
    void run();
    return () => {
      active = false;
      jobsRequest++;
      controller.abort();
    };
  }, [api, snap?.project.id]);
  useEffect(() => {
    if (!api || !frameRequest) return;
    const job = jobs.find((item) => item.job_id === frameRequest.jobId);
    if (!job) return;
    if (
      !snap ||
      !isCurrentProjectFrameRequest(frameRequest, {
        projectId: snap.project.id,
        revision: snap.revision,
        generation: frameGeneration.current,
      })
    ) {
      frameGeneration.current++;
      setFrameRequest(null);
      setStatus("Project changed; stale frame request was detached");
      return;
    }
    if (job.state === "failed" || job.state === "cancelled") {
      setFrameRequest(null);
      setStatus(
        job.state === "failed"
          ? `Project preview failed${jobErrorText(job.error) ? ` · ${jobErrorText(job.error)}` : ""}`
          : "Project preview cancelled",
      );
      return;
    }
    if (job.state !== "succeeded") return;
    if (frameArtifactDownload.current?.jobId === job.job_id) return;
    frameArtifactDownload.current?.controller.abort();
    const controller = new AbortController();
    frameArtifactDownload.current = { jobId: job.job_id, controller };
    const request = frameRequest;
    api
      .artifact(job.job_id, controller.signal)
      .then((blob) => {
        const current = frameContext.current;
        if (
          !isCurrentProjectFrameRequest(request, {
            projectId: current.projectId ?? "",
            revision: current.revision ?? "",
            generation: frameGeneration.current,
          }) ||
          current.api !== api ||
          current.historicalRevision !== null
        )
          return;
        const next = URL.createObjectURL(blob);
        setPreview((current) => {
          if (current) URL.revokeObjectURL(current);
          return next;
        });
        setPreviewType(job.kind === "preview.project_range" ? "video" : "image");
        const result =
            job.result && typeof job.result === "object"
              ? (job.result as Record<string, unknown>)
              : {},
          requested = resultTime(result.requested_time),
          actual = resultTime(result.actual_sequence_time);
        if (job.kind === "preview.project_range") {
          setPreviewLabel(rangePreviewLabel(job));
        } else {
          setPreviewLabel(
            `${job.kind === "preview.composited_frame" ? "COMPOSITED" : "ENCODED"} PROJECT FRAME · REV ${request.revision.slice(0, 12)} · REQUESTED ${requested ? n(requested).toFixed(3) : request.requestedTime}s · ACTUAL ${actual ? n(actual).toFixed(3) + "s" : "UNKNOWN"}`,
          );
        }
        setFrameRequest(null);
      })
      .catch((error) => {
        if (
          (error as Error).name !== "AbortError" &&
          frameGeneration.current === request.generation
        ) {
          setStatus((error as Error).message);
          setFrameRequest(null);
        }
      })
      .finally(() => {
        if (frameArtifactDownload.current?.controller === controller)
          frameArtifactDownload.current = null;
      });
  }, [api, frameRequest, jobs, snap]);
  const addCaption = () => {
    if (!seq || !captionText.trim()) return;
    const fontFamily = captionFontFamily.trim(),
      fontSize = Number(captionFontSize);
    if (!fontFamily || !Number.isFinite(fontSize) || fontSize <= 0) {
      setStatus(
        "Caption primary font is required and font size must be positive",
      );
      return;
    }
    const base = 1000;
    const style: CaptionStyle = {
      font_family: fontFamily,
      font_size: fontSize,
      color: "#ffffff",
      background_color: null,
      font_policy: captionFallback.trim()
        ? {
            mode: "explicit_fallback",
            families: captionFallback
              .split(",")
              .map((value) => value.trim())
              .filter(Boolean),
          }
        : { mode: "strict" },
      text_align: captionAlign,
      max_width: captionMaxWidth ? Number(captionMaxWidth) : null,
      line_height: captionLineHeight ? Number(captionLineHeight) : null,
    };
    apply("Insert caption", [
      {
        type: "caption.insert",
        sequence_id: seq.id,
        cue: {
          id: crypto.randomUUID(),
          timeline_range: {
            start: t(Math.round(Number(captionStart) * base), base),
            duration: t(Math.round(Number(captionDuration) * base), base),
          },
          text: captionText.trim(),
          source_anchor: null,
          style,
        },
      },
    ]).then((saved) => { if (saved) setCaptionText(""); });
  };
  const setKeyframeChannel = () => {
    if (!seq || !selected) return;
    try {
      const time = timeFromDecimalSeconds(keyframeTime),
        value = Number(keyframeValue),
        descriptor = parameters.find(
          (parameter) =>
            parameter.target === "clip" &&
            parameter.name === keyframeParameter &&
            parameter.animatable,
        );
      if (!descriptor) throw new Error("This parameter is not animatable");
      if (!Number.isFinite(value)) throw new Error("Keyframe value is invalid");
      if (
        (descriptor.minimum !== null &&
          descriptor.minimum !== undefined &&
          value < descriptor.minimum) ||
        (descriptor.maximum !== null &&
          descriptor.maximum !== undefined &&
          value > descriptor.maximum)
      )
        throw new Error("Keyframe value is outside the supported range");
      if (
        compareTime(time, t(0)) < 0 ||
        compareTime(time, selected.clip.timeline_range.duration) > 0
      )
        throw new Error("Clip-local time is outside the clip duration");
      if (!descriptor.interpolation.includes(keyframeInterpolation))
        throw new Error("Interpolation is not supported for this parameter");
      const channel = (selected.clip.channels ?? []).find(
          (item) => item.parameter === keyframeParameter,
        ),
        keyframes = [
          ...(channel?.keyframes.filter(
            (item) => compareTime(item.time, time) !== 0,
          ) ?? []),
          {
            id: crypto.randomUUID(),
            time,
            value,
            interpolation: keyframeInterpolation,
          },
        ].sort((left, right) => compareTime(left.time, right.time));
      void apply(`Set ${keyframeParameter} keyframe`, [
        {
          type: "keyframe.channel_set",
          sequence_id: seq.id,
          track_id: selected.track.id,
          clip_id: selected.clip.id,
          channel: { parameter: keyframeParameter, keyframes },
        },
      ]);
    } catch (error) {
      setStatus((error as Error).message);
    }
  };
  const runRenderPreflight = async (snapshot: Snapshot) => {
    if (!api || historical) return null;
    const generation = ++preflightGeneration.current,
      sequenceId = snapshot.project.default_sequence_id,
      identity = {
        projectId: snapshot.project.id,
        revision: snapshot.revision,
        sequenceId,
        generation,
      },
      controller = new AbortController();
    preflightAbort.current?.abort();
    preflightAbort.current = controller;
    setPreflightChecking(true);
    setPreflight(null);
    try {
      const report = await api.renderPreflight(
        {
          project_id: identity.projectId,
          revision: identity.revision,
          sequence_id: identity.sequenceId,
          container: "mp4",
          video_codec: "h264",
        },
        controller.signal,
      );
      const current = frameContext.current;
      if (
        current.api !== api ||
        current.historicalRevision !== null ||
        !isCurrentRenderRevision(identity, {
          projectId: current.projectId ?? "",
          revision: current.revision ?? "",
          sequenceId: current.sequenceId ?? "",
          generation: preflightGeneration.current,
        })
      )
        return null;
      if (
        report.revision !== identity.revision ||
        report.sequence_id !== identity.sequenceId
      ) {
        setStatus("Preflight response did not match the requested revision");
        return null;
      }
      setPreflight(report);
      setStatus(
        report.ready
          ? "Render preflight ready; this is a current environment check, not an input lock"
          : `Render preflight found ${report.issues.length} issue${report.issues.length === 1 ? "" : "s"}`,
      );
      return report;
    } catch (error) {
      if (
        (error as Error).name !== "AbortError" &&
        preflightGeneration.current === generation
      )
        setStatus(`Preflight: ${(error as Error).message}`);
      return null;
    } finally {
      if (preflightGeneration.current === generation) {
        setPreflightChecking(false);
        if (preflightAbort.current === controller)
          preflightAbort.current = null;
      }
    }
  };
  const requestProjectFrame = async (mode: "encoded" | "composited" | "range") => {
    if (!api || !snap || historical) return;
    const composited = mode === "composited";
    if (!(mode === "range" ? rangePreviewAvailable : composited ? compositedFrameAvailable : framePreviewAvailable)) {
      setStatus("This host does not support rendered project frame previews");
      return;
    }
    const checkedSnapshot = snap;
    const workspaceRequest = beginSourceRequest();
    const startInput = frameTime;
    const durationInput = rangeDuration;
    if (!workspaceRequest) return;
    // Full-export preflight includes audio dependencies. A visual-only frame
    // is validated by its own planner and must not depend on audio files.
    const prepared = await prepareCurrentPreviewRequest({
      request: workspaceRequest,
      current: currentSourceWorkspace,
      prepare: async () =>
        composited || (await runRenderPreflight(checkedSnapshot))?.ready === true,
    });
    if (!prepared) return;
    let generation: number | null = null;
    try {
      const requestedTime = timeFromDecimalSeconds(startInput),
        nextGeneration = ++frameGeneration.current,
        request = {
          projectId: snap.project.id,
          revision: snap.revision,
          generation: nextGeneration,
        };
      generation = nextGeneration;
      frameArtifactDownload.current?.controller.abort();
      frameArtifactDownload.current = null;
      setFrameRequest(null);
      setFrameCreating(true);
      const job = mode === "range"
        ? await api.previewProjectRange(checkedSnapshot, {
            start: requestedTime,
            duration: timeFromDecimalSeconds(durationInput),
          })
        : await api.previewProjectFrame(
            checkedSnapshot,
            requestedTime,
            composited ? "preview.composited_frame" : "preview.project_frame",
          );
      const current = frameContext.current;
      if (
        current.api !== api ||
        current.historicalRevision !== null ||
        !isCurrentProjectFrameRequest(request, {
          projectId: current.projectId ?? "",
          revision: current.revision ?? "",
          generation: frameGeneration.current,
        })
      ) {
        void api.cancelJob(job.job_id).catch(() => {});
        return;
      }
      setFrameRequest({
        jobId: job.job_id,
        ...request,
        requestedTime: startInput,
      });
      setJobs((current) => [
        job,
        ...current.filter((item) => item.job_id !== job.job_id),
      ]);
      setStatus("Project frame queued; rendering may take a while");
    } catch (error) {
      if (generation === null || frameGeneration.current === generation)
        setStatus((error as Error).message);
    } finally {
      if (generation === null || frameGeneration.current === generation)
        setFrameCreating(false);
    }
  };
  const exportProject = async () => {
    if (!api || !snap || historical) return;
    const checkedSnapshot = snap,
      report = await runRenderPreflight(checkedSnapshot);
    if (!report?.ready) return;
    const current = frameContext.current;
    if (
      current.api !== api ||
      current.projectId !== checkedSnapshot.project.id ||
      current.revision !== checkedSnapshot.revision ||
      current.historicalRevision !== null
    )
      return;
    const submission = {
      projectId: checkedSnapshot.project.id,
      revision: checkedSnapshot.revision,
      sequenceId: checkedSnapshot.project.default_sequence_id,
      generation: preflightGeneration.current,
    };
    try {
      const job = await api.exportProject(checkedSnapshot);
      const latest = frameContext.current;
      if (
        latest.api !== api ||
        latest.historicalRevision !== null ||
        !isCurrentRenderRevision(submission, {
          projectId: latest.projectId ?? "",
          revision: latest.revision ?? "",
          sequenceId: latest.sequenceId ?? "",
          generation: preflightGeneration.current,
        })
      ) {
        // Accepted exports remain durable for their fixed revision. Navigation
        // only prevents this stale response from entering the new UI context.
        return;
      }
      setJobs((jobs) => [
        job,
        ...jobs.filter((item) => item.job_id !== job.job_id),
      ]);
      setStatus(
        `Export queued for revision ${submission.revision.slice(0, 12)}`,
      );
    } catch (error) {
      if (preflightGeneration.current === submission.generation)
        setStatus(`Export: ${(error as Error).message}`);
    }
  };
  const downloadArtifact = async (job: Job) => {
    if (!api) return;
    const blob = await api.artifact(job.job_id),
      href = URL.createObjectURL(blob),
      a = document.createElement("a");
    a.href = href;
    a.download = `opencut-${job.job_id}.${isProjectFrameJob(job) ? "png" : "mp4"}`;
    a.click();
    URL.revokeObjectURL(href);
  };
  const previewArtifact = async (job: Job) => {
    const request = beginSourceRequest();
    if (!api || !request || request.connection !== api) return;
    const generation = frameGeneration.current;
    try {
      const blob = await api.artifact(job.job_id);
      if (generation !== frameGeneration.current ||
          !isCurrentWorkspace(request, currentSourceWorkspace())) return;
      const next = URL.createObjectURL(blob);
      setPreview((current) => {
        if (current) URL.revokeObjectURL(current);
        return next;
      });
      setPreviewType("video");
      const revision = (job.input as { snapshot?: { revision?: string } })
        ?.snapshot?.revision;
      setPreviewLabel(job.kind === "preview.project_range"
        ? rangePreviewLabel(job)
        : `RENDERED ARTIFACT · REV ${(revision ?? "UNKNOWN").slice(0, 12)}`);
    } catch (error) {
      if (generation === frameGeneration.current &&
          isCurrentWorkspace(request, currentSourceWorkspace()))
        setStatus((error as Error).message);
    }
  };
  const viewRevision = async (revision: string) => {
    if (!api || !snap) return;
    const generation = beginNavigation();
    try {
      const snapshot = await api.revision(snap.project.id, revision);
      if (!finishNavigation(generation)) return;
      setHistorical(snapshot);
      setSelectedIds(null);
      setStatus(`Read-only revision ${revision.slice(0, 12)}`);
    } catch (error) {
      if (finishNavigation(generation)) setStatus((error as Error).message);
    }
  };
  const restoreRevision = async (
    revision: string,
    label = "Restore entire project",
  ) => {
    if (!api || !snap) return;
    const previous = snap.revision;
    const restored = await act(label, () =>
      api.restore(snap.project.id, snap.revision, revision, label),
      true,
    );
    if (restored) {
      setHistorical(null);
      setRedoRevision(label === "Redo by restoring prior revision" ? null : previous);
    }
  };
  const refreshHistory = async () => {
    const request = headContext.current;
    if (!request) return;
    try {
      const page = await request.connection.history(request.projectId);
      if (isCurrentWorkspace(request, headContext.current)) {
        setHistory(page);
        if (page.head_revision !== request.revision) setRedoRevision(null);
      }
    } catch (error) {
      if (isCurrentWorkspace(request, headContext.current))
        setStatus((error as Error).message);
    }
  };
  const refreshProjects = async () => {
    const connection = api, generation = workspaceGeneration.current;
    if (!connection) return;
    try {
      const list = await connection.projects();
      if (generation === workspaceGeneration.current) setProjects(list);
    } catch (error) {
      if (generation === workspaceGeneration.current) setStatus((error as Error).message);
    }
  };
  const refreshJobList = async (cancelId?: string) => {
    const connection = api, generation = workspaceGeneration.current;
    if (!connection) return;
    try {
      if (cancelId) await connection.cancelJob(cancelId);
      const list = await connection.jobs();
      if (generation === workspaceGeneration.current) setJobs(list);
    } catch (error) {
      if (generation === workspaceGeneration.current) setStatus((error as Error).message);
    }
  };
  const returnToHead = () => {
    const generation = beginNavigation();
    setHistorical(null);
    finishNavigation(generation);
  };
  const undoLatest = () => {
    const head = history?.revisions.find(
      (item) => item.revision === snap?.revision,
    );
    if (!head?.parent_revision) {
      setStatus("No parent revision to restore");
      return;
    }
    return restoreRevision(
      head.parent_revision,
      "Undo latest action by restoring parent revision",
    );
  };
  return (
    <div className="oc-shell">
      <header className="oc-top">
        <div className="oc-mark">
          <Clapperboard />
          <b>OPENCUT</b>
          <span>EDIT / 01</span>
        </div>
        <div className="oc-connect">
          <input
            aria-label="API address"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
          />
          <input
            aria-label="Bearer token"
            type="password"
            placeholder="Bearer token"
            value={token}
            onChange={(e) => setToken(e.target.value)}
          />
          <button onClick={connect} disabled={busy || transitioning}>
            {client ? <Wifi /> : <WifiOff />}
            {status}
          </button>
        </div>
      </header>
      <aside className="oc-library">
        <div className="panel-title">
          <span>PROJECTS</span>
          <button
            aria-label="Refresh projects"
            onClick={refreshProjects}
          >
            <RefreshCw />
          </button>
        </div>
        {projects.map((p) => (
          <button
            className={
              snap?.project.id === p.project.id ? "project active" : "project"
            }
            onClick={() => choose(p.project.id)}
            key={p.project.id}
          >
            <FolderOpen />
            <span>
              {p.project.name}
              <small>{p.revision.slice(0, 12)}</small>
            </span>
          </button>
        ))}
        <button className="create" onClick={createProject} disabled={!api || busy || transitioning}>
          <Plus /> New project
        </button>
        <label className="upload">
          <CloudUpload />
          Import media
          <input
            type="file"
            accept="video/*,audio/*,image/*"
            onChange={(e) =>
              e.target.files?.[0] && importMedia(e.target.files[0])
            }
          />
        </label>
        <div className="asset-list">
          {assets.map((asset) => {
            const version = assetVersionView(asset),
              replaceable = ["video", "audio", "image"].includes(asset.kind);
            return (
              <div className="asset-row" key={asset.id}>
                <button
                  onClick={() => asset.uri && setStatus(`Asset: ${asset.name}`)}
                >
                  <Film />
                  <span>
                    {asset.name}
                    <small>
                      {asset.kind} ·{" "}
                      {asset.duration
                        ? n(asset.duration).toFixed(1) + "s"
                        : "still"}
                    </small>
                    <small className={version ? "asset-fixed" : "asset-legacy"}>
                      {version
                        ? `FIXED ${version.contentHash.slice(0, 12)} · ${version.size.toLocaleString()} B`
                        : "LEGACY · MEDIA BYTES NOT REVISION-FIXED"}
                    </small>
                  </span>
                </button>
                <label
                  className={`replace-media${replaceable ? "" : " disabled"}`}
                >
                  {replaceable ? "Replace media" : "Replace unsupported"}
                  <input
                    type="file"
                    disabled={busy || Boolean(historical) || !replaceable}
                    accept={
                      asset.kind === "audio"
                        ? "audio/*"
                        : asset.kind === "image"
                          ? "image/*"
                          : asset.kind === "video"
                            ? "video/*"
                            : undefined
                    }
                    onChange={(event) => {
                      const file = event.target.files?.[0];
                      if (file) void replaceMedia(asset, file);
                      event.currentTarget.value = "";
                    }}
                  />
                </label>
              </div>
            );
          })}
        </div>
      </aside>
      <main className="oc-stage">
        <div className="viewer">
          <div className="viewer-head">
            <span>{previewLabel}</span>
            <em>
              {seq
                ? `${seq.width}×${seq.height} · ${seq.fps.numerator}/${seq.fps.denominator} FPS`
                : "NO SEQUENCE"}
            </em>
          </div>
          {preview ? (
            previewType === "image" ? (
              <img src={preview} alt={previewLabel} />
            ) : (
              <video src={preview} controls />
            )
          ) : (
            <div className="empty-view">
              <Film />
              <b>Source preview</b>
              <span>
                Import or select authorized media. This is not a rendered
                project preview.
              </span>
            </div>
          )}
          <div className="transport">
            <label>
              Start / frame
              <input
                aria-label="Project frame time in seconds"
                inputMode="decimal"
                value={frameTime}
                onChange={(event) => setFrameTime(event.target.value)}
              />
              s
            </label>
            <button
              disabled={
                !snap ||
                Boolean(historical) ||
                !framePreviewAvailable ||
                preflightChecking ||
                frameCreating ||
                Boolean(frameRequest)
              }
              onClick={() => requestProjectFrame("encoded")}
            >
              {frameCreating
                ? "Queuing frame…"
                : frameRequest
                  ? "Rendering frame…"
                  : "Preview encoded frame"}
            </button>
            <button
              disabled={
                !snap || Boolean(historical) || !compositedFrameAvailable ||
                preflightChecking || frameCreating || Boolean(frameRequest)
              }
              onClick={() => requestProjectFrame("composited")}
              title="Compose a PNG directly. Long sources may still take time to decode."
            >
              Preview composited frame
            </button>
            <label>
              Duration
              <input
                aria-label="Project preview duration in seconds"
                inputMode="decimal"
                value={rangeDuration}
                onChange={(event) => setRangeDuration(event.target.value)}
              />
              s
            </label>
            <button
              disabled={
                !snap || Boolean(historical) || !rangePreviewAvailable ||
                preflightChecking || frameCreating || Boolean(frameRequest)
              }
              onClick={() => requestProjectFrame("range")}
              title="Preview the selected sequence interval with sound."
            >
              Preview range with audio
            </button>
            <small className="frame-cost">
              {compositedFrameAvailable
                ? "Composited preview creates an image directly. Encoded preview renders the full sequence first."
                : framePreviewAvailable
                  ? "Encoded preview renders the full sequence first and may take time."
                  : "PROJECT FRAME PREVIEW UNAVAILABLE ON THIS HOST"}
            </small>
            <span>
              {previewLabel === "SOURCE MONITOR"
                ? "SOURCE · NATIVE CONTROLS"
                : previewLabel}
            </span>
          </div>
        </div>
      </main>
      <aside className="oc-inspector">
        <div className="panel-title">INSPECTOR</div>
        {selected ? (
          <>
            <label>
              Clip ID
              <input readOnly value={selected.clip.id} />
            </label>
            {parameters
              .filter((parameter) => parameter.target === "clip")
              .map((parameter) => (
                <label key={parameter.name}>
                  {parameter.name.replaceAll("_", " ")}
                  <small>{parameter.unit ?? parameter.value_type}</small>
                  <input
                    type="number"
                    min={parameter.minimum ?? undefined}
                    max={parameter.maximum ?? undefined}
                    step="0.05"
                    defaultValue={Number(
                      selected.clip.params?.[parameter.name] ??
                        parameter.default_value ??
                        0,
                    )}
                    disabled={Boolean(historical)}
                    onBlur={(event) =>
                      seq &&
                      apply(`Set ${parameter.name}`, [
                        {
                          type: "parameter.set_clip",
                          sequence_id: seq.id,
                          track_id: selected.track.id,
                          clip_id: selected.clip.id,
                          params: {
                            [parameter.name]: Number(event.currentTarget.value),
                          },
                        },
                      ])
                    }
                  />
                </label>
              ))}
            <div className="keyframe-editor">
              <div className="keyframe-status">
                <b>KEYFRAMES</b>
                <small>
                  {capabilities?.rendering.keyframes
                    ? "Rendered in project output"
                    : "Stored only · renderer unavailable"}
                </small>
              </div>
              <select
                aria-label="Animated parameter"
                value={keyframeParameter}
                onChange={(event) => setKeyframeParameter(event.target.value)}
              >
                {parameters
                  .filter(
                    (parameter) =>
                      parameter.target === "clip" && parameter.animatable,
                  )
                  .map((parameter) => (
                    <option value={parameter.name} key={parameter.name}>
                      {parameter.name}
                    </option>
                  ))}
              </select>
              <label>
                Clip-local time (s)
                <input
                  inputMode="decimal"
                  value={keyframeTime}
                  onChange={(event) => setKeyframeTime(event.target.value)}
                />
              </label>
              <label>
                Value
                <input
                  type="number"
                  step="0.05"
                  value={keyframeValue}
                  onChange={(event) => setKeyframeValue(event.target.value)}
                />
              </label>
              <label>
                Interpolation
                <select
                  value={keyframeInterpolation}
                  onChange={(event) =>
                    setKeyframeInterpolation(
                      event.target.value as "hold" | "linear",
                    )
                  }
                >
                  <option value="hold">Hold</option>
                  <option value="linear">Linear</option>
                </select>
              </label>
              <button
                disabled={Boolean(historical)}
                onClick={setKeyframeChannel}
              >
                Add / replace keyframe
              </button>
              {(selected.clip.channels ?? []).map((channel) => (
                <div className="keyframe-channel" key={channel.parameter}>
                  <span>
                    {channel.parameter} · {channel.keyframes.length} keys
                  </span>
                  <button
                    disabled={Boolean(historical)}
                    onClick={() =>
                      seq &&
                      apply(`Delete ${channel.parameter} keyframes`, [
                        {
                          type: "keyframe.channel_delete",
                          sequence_id: seq.id,
                          track_id: selected.track.id,
                          clip_id: selected.clip.id,
                          parameter: channel.parameter,
                        },
                      ])
                    }
                  >
                    Delete channel
                  </button>
                </div>
              ))}
            </div>
            <div className="action-row">
              <button onClick={split}>
                <Scissors />
                Split
              </button>
              <button onClick={trim}>Trim −1s</button>
              <button className="danger" onClick={del}>
                <Trash2 />
                Delete
              </button>
            </div>
          </>
        ) : (
          <p className="muted">
            Select a clip in the timeline to edit exact properties.
          </p>
        )}
        <div className="panel-title">CAPTION</div>
        <div className="caption-form">
          <textarea
            aria-label="Caption text"
            placeholder="Caption text"
            value={captionText}
            onChange={(e) => setCaptionText(e.target.value)}
          />
          <label>
            Start (s)
            <input
              value={captionStart}
              onChange={(e) => setCaptionStart(e.target.value)}
            />
          </label>
          <label>
            Duration (s)
            <input
              value={captionDuration}
              onChange={(e) => setCaptionDuration(e.target.value)}
            />
          </label>
          <label>
            Primary font
            <input
              required
              value={captionFontFamily}
              onChange={(event) => setCaptionFontFamily(event.target.value)}
            />
          </label>
          <label>
            Font size (px)
            <input
              required
              type="number"
              min="0.01"
              step="0.01"
              value={captionFontSize}
              onChange={(event) => setCaptionFontSize(event.target.value)}
            />
          </label>
          <label>
            Align
            <select
              value={captionAlign}
              onChange={(event) =>
                setCaptionAlign(event.target.value as typeof captionAlign)
              }
            >
              <option value="left">Left</option>
              <option value="center">Center</option>
              <option value="right">Right</option>
            </select>
          </label>
          <label>
            Fallback fonts
            <input
              placeholder="Optional, comma separated"
              value={captionFallback}
              onChange={(event) => setCaptionFallback(event.target.value)}
            />
          </label>
          <label>
            Max width (px)
            <input
              type="number"
              min="1"
              value={captionMaxWidth}
              onChange={(event) => setCaptionMaxWidth(event.target.value)}
            />
          </label>
          <label>
            Line height (px)
            <input
              type="number"
              min="1"
              value={captionLineHeight}
              onChange={(event) => setCaptionLineHeight(event.target.value)}
            />
          </label>
          <button
            disabled={
              !captionFontFamily.trim() ||
              !Number.isFinite(Number(captionFontSize)) ||
              Number(captionFontSize) <= 0
            }
            onClick={addCaption}
          >
            Insert caption
          </button>
        </div>
        {(seq?.caption_cues ?? []).map((c) => (
          <div className="cue-row" key={c.id}>
            <span>{c.text}</span>
            <button
              onClick={() =>
                apply("Delete caption", [
                  {
                    type: "caption.delete",
                    sequence_id: seq!.id,
                    cue_id: c.id,
                  },
                ])
              }
            >
              <Trash2 />
            </button>
          </div>
        ))}
        <div className="panel-title">RENDER READINESS</div>
        <div className="preflight-panel">
          <button
            disabled={!api || !snap || Boolean(historical) || preflightChecking}
            onClick={() => snap && void runRenderPreflight(snap)}
          >
            {preflightChecking ? "Checking…" : "Check current revision"}
          </button>
          <p>
            Read-only environment check for this revision. It does not lock
            media, fonts, plugins, or tools for a later job.
          </p>
          {preflight && (
            <div className={preflight.ready ? "ready" : "blocked"}>
              <b>{preflight.ready ? "READY" : "NOT READY"}</b>
              <small>
                REV {preflight.revision.slice(0, 12)} · SEQ{" "}
                {preflight.sequence_id.slice(0, 8)}
              </small>
              {preflight.issues.map((issue, index) => (
                <div className="preflight-issue" key={`${issue.code}-${index}`}>
                  <b>{issue.code}</b>
                  <span>{issue.message}</span>
                  <small>
                    {[
                      issue.asset_id && `ASSET ${issue.asset_id}`,
                      issue.clip_id && `CLIP ${issue.clip_id}`,
                      issue.cue_id && `CUE ${issue.cue_id}`,
                    ]
                      .filter(Boolean)
                      .join(" · ") || "PROJECT / HOST"}
                  </small>
                </div>
              ))}
            </div>
          )}
        </div>
        <div className="panel-title">
          JOBS{" "}
          <button onClick={() => refreshJobList()}>
            <RefreshCw />
          </button>
        </div>
        <button
          className="export"
          disabled={!snap || Boolean(historical) || preflightChecking}
          onClick={exportProject}
        >
          Export revision
        </button>
        <div className="jobs">
          {jobs.map((job) => {
            const lock = mediaAssetLock(job.input);
            return (
              <div key={job.job_id}>
                <b>{job.kind}</b>
                <span>{job.state}</span>
                {progressOf(job) && (
                  <>
                    <small>
                      {progressOf(job)!.stage}
                      {progressOf(job)!.fraction === null
                        ? ""
                        : ` · ${Math.round(progressOf(job)!.fraction! * 100)}%`}
                    </small>
                    {progressOf(job)!.fraction !== null && (
                      <progress max="1" value={progressOf(job)!.fraction!} />
                    )}
                  </>
                )}
                {job.state === "succeeded" && (
                  <>
                    {!isProjectFrameJob(job) && (
                      <button onClick={() => previewArtifact(job)}>
                        Preview
                      </button>
                    )}
                    <button onClick={() => downloadArtifact(job)}>
                      Download
                    </button>
                  </>
                )}
                {(job.state === "queued" || job.state === "running") && (
                  <button
                    onClick={() => refreshJobList(job.job_id)}
                  >
                    Cancel
                  </button>
                )}
                <small>{job.job_id.slice(0, 12)}</small>
                {lock && (
                  <div className="asset-lock">
                    <b>{lock.assets.length} MEDIA ASSETS LOCKED</b>
                    <small>
                      REV {lock.revision.slice(0, 12)} · SEQ{" "}
                      {lock.sequenceId.slice(0, 8)}
                    </small>
                    <span>
                      Job input media only; fonts, plugins, tools, and unrelated
                      sources are not covered.
                    </span>
                    {lock.assets.map((asset) => (
                      <small key={asset.assetId}>
                        {asset.assetId} · {asset.size.toLocaleString()} B ·{" "}
                        {asset.contentHash.slice(0, 12)}
                      </small>
                    ))}
                  </div>
                )}
                {!lock &&
                  capabilities?.job_execution.project_asset_locking &&
                  (job.kind === "render.project" || job.kind === "preview.project_range" ||
                    isProjectFrameJob(job)) && (
                    <div className="asset-lock missing">
                      <b>MEDIA ASSET LOCK NOT RECORDED</b>
                      <span>
                        This job record does not expose a valid version 1 media
                        lock manifest. No lock is claimed.
                      </span>
                    </div>
                  )}
              </div>
            );
          })}
        </div>
        <div className="job-events">
          {jobEvents.slice(0, 12).map((event) => (
            <div key={event.sequence}>
              <b>{event.event_type}</b>
              <span>{event.state ?? "event"}</span>
              <small>
                #{event.sequence} · {event.job_id.slice(0, 8)}
              </small>
            </div>
          ))}
        </div>
        <div className="panel-title">
          HISTORY{" "}
          <button
            aria-label="Refresh history"
            onClick={refreshHistory}
          >
            <RefreshCw />
          </button>
        </div>
        <div className="history-actions">
          <button
            disabled={
              busy || transitioning || !history?.revisions.find(
                (item) => item.revision === snap?.revision,
              )?.parent_revision
            }
            onClick={undoLatest}
          >
            Undo latest action
          </button>
          <button
            disabled={!redoRevision || busy || transitioning}
            onClick={() =>
              redoRevision &&
              restoreRevision(redoRevision, "Redo by restoring prior revision")
            }
          >
            Redo restore
          </button>
        </div>
        {historical && (
          <div className="history-warning">
            <b>READ-ONLY HISTORICAL SNAPSHOT</b>
            <span>
              Viewing {historical.revision.slice(0, 12)}. Restore replaces the
              entire current project state and creates a new revision.
            </span>
            <button onClick={returnToHead}>Return to head</button>
            <button
              disabled={busy || transitioning}
              onClick={() =>
                restoreRevision(
                  historical.revision,
                  "Restore entire project to selected revision",
                )
              }
            >
              Restore this revision
            </button>
          </div>
        )}
        <div className="history-list">
          {history?.revisions.map((item) => (
            <div
              className={item.revision === snap?.revision ? "head" : ""}
              key={item.revision}
            >
              <button onClick={() => viewRevision(item.revision)}>
                <b>{item.label || "Project revision"}</b>
                <small>
                  {item.revision.slice(0, 12)}
                  {item.revision === snap?.revision ? " · HEAD" : ""}
                </small>
              </button>
              {item.revision !== snap?.revision && (
                <button
                  onClick={() =>
                    restoreRevision(
                      item.revision,
                      "Restore entire project to selected revision",
                    )
                  }
                >
                  Restore
                </button>
              )}
            </div>
          ))}
        </div>
        <div className="panel-title">ACTIVITY</div>
        <div className="activity">
          {log.map((x, i) => (
            <div className={x.ok ? "ok" : "fail"} key={i}>
              <i />
              {x.label}
              <small>{x.at}</small>
            </div>
          ))}
        </div>
      </aside>
      <section className="oc-timeline">
        <div className="timeline-tools">
          <b>TIMELINE</b>
          <button onClick={addTrack}>
            <Plus />
            Track
          </button>
          <span>
            {shown
              ? `${historical ? "VIEWING " : "REV "}${shown.revision.slice(0, 10)}`
              : "NO PROJECT"}
          </span>
        </div>
        <div className="ruler">
          {[0, 0.25, 0.5, 0.75, 1].map((x) => (
            <i key={x} style={{ left: `${x * 100}%` }}>
              {(duration * x).toFixed(1)}s
            </i>
          ))}
        </div>
        {seq?.tracks.map((track) => (
          <div className="track" key={track.id}>
            <button
              className="track-head"
              onClick={() =>
                seq &&
                apply(track.muted ? "Unmute track" : "Mute track", [
                  {
                    type: "track.update",
                    sequence_id: seq.id,
                    track_id: track.id,
                    name: null,
                    enabled: null,
                    muted: !track.muted,
                    locked: null,
                  },
                ])
              }
            >
              <b>{track.name}</b>
              <small>
                {track.kind}
                {track.muted ? " · MUTED" : ""}
              </small>
            </button>
            <div className="track-lane">
              {track.clips.map((clip) => (
                <button
                  key={clip.id}
                  className={
                    selected?.clip.id === clip.id ? "clip selected" : "clip"
                  }
                  onClick={() =>
                    setSelectedIds({ trackId: track.id, clipId: clip.id })
                  }
                  style={{
                    left: `${(n(clip.timeline_range.start) / duration) * 100}%`,
                    width: `${Math.max(2, (n(clip.timeline_range.duration) / duration) * 100)}%`,
                  }}
                >
                  <b>
                    {assets.find((a) => a.id === clip.asset_id)?.name ??
                      clip.id.slice(0, 8)}
                  </b>
                  <small>{n(clip.timeline_range.duration).toFixed(2)}s</small>
                </button>
              ))}
            </div>
          </div>
        ))}
        {seq && (
          <div className="captions">
            {(seq.caption_cues ?? []).map((c) => (
              <span key={c.id}>{c.text}</span>
            ))}
          </div>
        )}
      </section>
      <section className="oc-raw">
        <div className="panel-title">
          RAW PLAN JSON <span>VALIDATED BEFORE APPLY</span>
        </div>
        <textarea value={raw} onChange={(e) => setRaw(e.target.value)} />
        <div>
          <button
            onClick={() => {
              try {
                const ops: unknown = JSON.parse(raw);
                if (!Array.isArray(ops))
                  throw new Error("operations must be an array");
                if (seq) apply("Apply raw plan", ops as Operation[]);
              } catch {
                setStatus("Invalid JSON");
              }
            }}
          >
            Apply transaction
          </button>
        </div>
      </section>
    </div>
  );
}
