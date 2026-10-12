import { getProjectId, isRecord } from "./legacy/utils";
import type { ClassicSnapshot } from "./decode";
import type { ProjectRecord } from "./legacy/types";
import { legacyId } from "./identity";
export type ClassicSource = {
  id: string;
  name: string;
  version: number;
  error?: string;
};
// Reviewed input adapter: opens existing legacy stores with readonly transactions.
// An absent database's upgrade transaction is aborted, never creating a new store.
// No deleteDatabase, put, clear, readwrite, version upgrade or OPFS create is used.
export async function readLegacyStore(
  name: string,
  store: string,
  signal?: AbortSignal,
): Promise<ProjectRecord[]> {
  signal?.throwIfAborted();
  return new Promise((resolve, reject) => {
    const open = indexedDB.open(name);
    let absent = false,
      settled = false;
    let database: IDBDatabase | undefined;
    const finish = (rows?: ProjectRecord[], error?: unknown) => {
      if (settled) return;
      settled = true;
      clearTimeout(timeout);
      signal?.removeEventListener("abort", aborted);
      database?.close();
      if (error !== undefined) reject(error);
      else resolve(rows ?? []);
    };
    const aborted = () =>
      finish(undefined, signal?.reason ?? new Error("Legacy read cancelled"));
    const timeout = setTimeout(
      () =>
        finish(
          undefined,
          new Error("Legacy read timed out; close old project tabs and retry"),
        ),
      15000,
    );
    signal?.addEventListener("abort", aborted, { once: true });
    open.onupgradeneeded = () => {
      absent = true;
      open.transaction?.abort();
    };
    open.onerror = () => (absent ? finish([]) : finish(undefined, open.error));
    open.onblocked = () =>
      finish(
        undefined,
        new Error(
          "Close other old project tabs before reading the legacy database.",
        ),
      );
    open.onsuccess = () => {
      const db = open.result;
      database = db;
      if (settled) {
        db.close();
        return;
      }
      db.onversionchange = () => db.close();
      if (!db.objectStoreNames.contains(store)) {
        db.close();
        finish([]);
        return;
      }
      const transaction = db.transaction(store, "readonly"),
        request = transaction.objectStore(store).getAll(undefined, 1001);
      transaction.oncomplete = () => {
        db.close();
        finish(request.result ?? []);
      };
      transaction.onabort = () => {
        db.close();
        finish(
          undefined,
          transaction.error ?? new Error("Legacy read interrupted"),
        );
      };
      request.onerror = () => {
        db.close();
        finish(undefined, request.error);
      };
    };
  });
}
const safeId = (id: string) => {
  if (!id || id.length > 512 || /[\/\\\u0000-\u001f]/.test(id))
    throw new Error("Invalid Classic storage ID");
  return id;
};
export async function discoverClassic(
  signal?: AbortSignal,
): Promise<ClassicSource[]> {
  if (typeof indexedDB === "undefined")
    throw new Error("This browser cannot read Classic IndexedDB projects.");
  const rows = await readLegacyStore(
    "video-editor-projects",
    "projects",
    signal,
  );
  if (rows.length > 1000)
    throw new Error("Too many Classic projects; export in smaller groups.");
  return rows.map((row, i) => {
    if (!isRecord(row))
      return {
        id: `invalid-${i}`,
        name: "Damaged project",
        version: -1,
        error: "Classic project record is damaged",
      };
    const id = getProjectId({ project: row });
    const metadata = isRecord(row.metadata) ? row.metadata : row;
    return {
      id: id ?? `invalid-${i}`,
      name:
        typeof metadata.name === "string"
          ? metadata.name
          : "Untitled Classic project",
      version: typeof row.version === "number" ? row.version : 0,
      ...(!id ? { error: "Classic project has no ID" } : {}),
    };
  });
}
export async function captureClassic(
  sourceId: string,
  signal: AbortSignal,
): Promise<ClassicSnapshot> {
  signal.throwIfAborted();
  safeId(sourceId);
  const rows = await readLegacyStore(
    "video-editor-projects",
    "projects",
    signal,
  );
  const raw = rows.find(
    (row) => isRecord(row) && getProjectId({ project: row }) === sourceId,
  );
  if (!raw) throw new Error("Classic project no longer exists");
  const media = await readLegacyStore(
    `video-editor-media-${sourceId}`,
    "media-metadata",
    signal,
  );
  if (media.length > 256)
    throw new Error(
      "Classic recovery supports at most 256 media assets per project",
    );
  const legacyTracksBySceneId: Record<string, unknown[]> = Object.create(null);
  if (raw.version === undefined || Number(raw.version) <= 1) {
    const scenes =
      Array.isArray(raw.scenes) && raw.scenes.length
        ? raw.scenes
        : [{ id: legacyId("v0-main", sourceId), isMain: true }];
    for (const scene of scenes) {
      if (!isRecord(scene) || typeof scene.id !== "string")
        throw new Error("Damaged Classic scene");
      safeId(scene.id);
      let data = await readLegacyStore(
        `video-editor-timelines-${sourceId}-${scene.id}`,
        "timeline",
        signal,
      );
      if (!data.length && scene.isMain === true)
        data = await readLegacyStore(
          `video-editor-timelines-${sourceId}`,
          "timeline",
          signal,
        );
      const timeline = data.find((row) => row.id === "timeline") ?? data[0];
      if (timeline && !Array.isArray(timeline.tracks))
        throw new Error("Damaged Classic timeline");
      legacyTracksBySceneId[scene.id] = (timeline?.tracks as unknown[]) ?? [];
    }
  }
  const mediaTypesById: ClassicSnapshot["context"]["mediaTypesById"] =
    Object.create(null);
  for (const row of media) {
    if (
      !isRecord(row) ||
      typeof row.id !== "string" ||
      !["video", "audio", "image"].includes(String(row.type))
    )
      throw new Error("Damaged Classic media metadata");
    safeId(row.id);
    if (mediaTypesById[row.id]) throw new Error("Duplicate Classic media ID");
    mediaTypesById[row.id] = row.type as "video" | "audio" | "image";
  }
  signal.throwIfAborted();
  return {
    origin: location.origin,
    sourceId,
    raw,
    media,
    context: { legacyTracksBySceneId, mediaTypesById },
  };
}
export async function readClassicFile(
  sourceId: string,
  media: ProjectRecord,
): Promise<File> {
  safeId(sourceId);
  if (typeof media.id !== "string") throw new Error("Invalid Classic media ID");
  safeId(media.id);
  const root = await navigator.storage.getDirectory();
  const directory = await root.getDirectoryHandle(`media-files-${sourceId}`);
  const file = await (await directory.getFileHandle(media.id)).getFile();
  if (!file.size) throw new Error(`Classic media is empty: ${media.id}`);
  if (typeof media.size === "number" && media.size !== file.size)
    throw new Error(`Classic media size changed: ${media.id}`);
  return new File(
    [file],
    typeof media.name === "string" ? media.name : file.name,
    { type: file.type, lastModified: file.lastModified },
  );
}
