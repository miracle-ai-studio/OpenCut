import { transforms, v0 } from "./transforms";
import { getProjectId, isRecord } from "./legacy/utils";
import type { ProjectRecord } from "./legacy/types";
import type { V1ToV2Context } from "./legacy/v1-to-v2";
import { digest, legacyId } from "./identity";
export type ClassicSnapshot = {
  origin: string;
  sourceId: string;
  raw: ProjectRecord;
  context: V1ToV2Context;
  media: ProjectRecord[];
};
export type DecodedClassic = {
  content: string;
  sourceVersion: number;
  sourceDigest: string;
  steps: number[];
  notes: string[];
  project: ProjectRecord;
};
// Classic's historic decoders filtered malformed/unknown tracks. Recovery must
// reject a damaged source rather than quietly publish a smaller document.
function elementIds(project: ProjectRecord, context?: V1ToV2Context): string[] {
  const ids: string[] = [];
  const scenes = Array.isArray(project.scenes) ? project.scenes : [];
  for (const scene of scenes) {
    if (!isRecord(scene) || typeof scene.id !== "string")
      throw new Error("Damaged Classic scene");
    let tracks = scene.tracks;
    if (context && (!Array.isArray(tracks) || !tracks.length))
      tracks = context.legacyTracksBySceneId[scene.id] ?? [];
    if (!Array.isArray(tracks) && !isRecord(tracks))
      throw new Error("Damaged Classic tracks");
    const values = Array.isArray(tracks)
      ? tracks
      : isRecord(tracks)
        ? Object.values(tracks).flat()
        : [];
    for (const track of values) {
      if (
        !isRecord(track) ||
        typeof track.id !== "string" ||
        !Array.isArray(track.elements)
      )
        throw new Error("Damaged Classic track");
      if (context) {
        if (
          !["media", "text", "audio"].includes(String(track.type)) ||
          ["muted", "hidden"].some(
            (key) => track[key] !== undefined && track[key] !== false,
          )
        )
          throw new Error(
            "Unsupported legacy track semantics would be discarded",
          );
        const allowedTrack = new Set([
          "id",
          "name",
          "type",
          "elements",
          "isMain",
          "muted",
          "hidden",
        ]);
        if (Object.keys(track).some((key) => !allowedTrack.has(key)))
          throw new Error(
            "Unsupported legacy track field; export the source without upgrading it",
          );
      }
      for (const element of track.elements) {
        if (!isRecord(element) || typeof element.id !== "string" || !element.id)
          throw new Error("Damaged Classic element");
        if (context) {
          if (element.volume !== undefined && element.volume !== 1)
            throw new Error("Unsupported legacy volume would be discarded");
          const common = [
            "id",
            "name",
            "type",
            "mediaId",
            "duration",
            "startTime",
            "trimStart",
            "trimEnd",
          ];
          const known =
            element.type === "text"
              ? [
                  ...common,
                  "content",
                  "fontSize",
                  "fontFamily",
                  "color",
                  "backgroundColor",
                  "textAlign",
                  "fontWeight",
                  "fontStyle",
                  "textDecoration",
                  "x",
                  "y",
                  "rotation",
                  "opacity",
                ]
              : [...common, "muted", "volume"];
          if (Object.keys(element).some((key) => !known.includes(key)))
            throw new Error(
              "Unsupported legacy element field; export the source without upgrading it",
            );
        }
        ids.push(`${scene.id}/${element.id}`);
      }
    }
  }
  return ids.sort();
}
// This reads historical serialization only. It never creates an OpenCut project,
// applies an edit or writes old stores. Native analysis/commit owns all new semantics.
export function decodeClassic(snapshot: ClassicSnapshot): DecodedClassic {
  const bytes = JSON.stringify(snapshot, (_key, value) => {
    if (typeof value === "number" && !Number.isFinite(value))
      throw new Error("Nonfinite Classic snapshot value; source is damaged");
    return value;
  });
  if (new TextEncoder().encode(bytes).length > 4 * 1024 * 1024)
    throw new Error("Classic snapshot exceeds 4 MiB");
  let project = structuredClone(snapshot.raw);
  if (!isRecord(project) || getProjectId({ project }) !== snapshot.sourceId)
    throw new Error("Classic source identity changed");
  const version = project.version === undefined ? 0 : project.version;
  if (
    !Number.isSafeInteger(version) ||
    Number(version) < 0 ||
    Number(version) > 31
  )
    throw new Error("Unsupported Classic project version");
  const notes: string[] = [],
    steps: number[] = [];
  if (version === 0) {
    const result = v0({
      project,
      options: {
        sceneId: legacyId("v0-main", snapshot.sourceId),
        now: new Date(0),
      },
    });
    if (result.skipped)
      throw new Error(`Classic v0 cannot be decoded: ${result.reason}`);
    project = result.project;
    steps.push(1);
  }
  for (let from = Math.max(1, Number(version)); from < 31; from++) {
    const before = elementIds(
      project,
      from === 1 ? snapshot.context : undefined,
    );
    const transform = transforms[from - 1];
    const result = transform({ project, context: snapshot.context });
    if (result.skipped || result.project.version !== from + 1)
      throw new Error(`Classic upgrade stopped at v${from}: ${result.reason}`);
    project = result.project;
    if (JSON.stringify(before) !== JSON.stringify(elementIds(project)))
      throw new Error(
        `Classic v${from} conversion would discard source elements`,
      );
    steps.push(from + 1);
  }
  // IndexedDB's keyPath adds this redundant field even to current v31 records.
  if (project.id !== undefined) {
    if (!isRecord(project.metadata) || project.id !== project.metadata.id)
      throw new Error("Conflicting Classic keyPath identity");
    delete project.id;
    notes.push(
      "Redundant IndexedDB keyPath ID is excluded from the serialized document; original snapshot is retained.",
    );
  }
  const content = JSON.stringify(project);
  if (new TextEncoder().encode(content).length > 1024 * 1024)
    throw new Error("Classic project exceeds the native 1 MiB limit");
  return {
    content,
    project,
    sourceVersion: Number(version),
    sourceDigest: digest(bytes),
    steps,
    notes,
  };
}
