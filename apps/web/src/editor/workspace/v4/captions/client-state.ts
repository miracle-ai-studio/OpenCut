import type { ToolInput } from "../../../../lib/editor-sdk";
import {
  canonicalRecoveryIdentity,
  validRecoveryId,
} from "../edit-recovery-pointers";
export type TranscriptionRequestV4 =
  ToolInput<"opencut_analysis_sequence_transcribe_v4">;
type RecoveryEntry = {
  kind: "caption_recovery";
  identity: string;
  projectId: string;
  recoveryId: string;
};
type TranscriptionEntry = {
  kind: "transcription";
  identity: string;
  projectId: string;
  sceneId: string;
  request: TranscriptionRequestV4;
};
type Entry = RecoveryEntry | TranscriptionEntry;
type Envelope = { schemaVersion: 1; entries: Entry[] };
const invalid = () =>
  new Error(
    "Caption client recovery storage is unavailable or invalid. Resolve the original request before continuing.",
  );
const plain = (value: unknown): value is Record<string, unknown> =>
  !!value && typeof value === "object" && !Array.isArray(value);
function keys(value: Record<string, unknown>, expected: string) {
  if (Object.keys(value).sort().join() !== expected) throw invalid();
}
function id(value: unknown): asserts value is string {
  if (!validRecoveryId(value) || /[\\/]/.test(value)) throw invalid();
}
function identity(value: unknown): asserts value is string {
  if (typeof value !== "string" || canonicalRecoveryIdentity(value) !== value)
    throw invalid();
}
function request(
  value: unknown,
  project: string,
  scene: string,
): asserts value is TranscriptionRequestV4 {
  if (!plain(value)) throw invalid();
  const names = Object.keys(value).sort().join();
  if (
    names !== "idempotency_key,language,project_id,revision,sequence_id" &&
    names !== "idempotency_key,project_id,revision,sequence_id"
  )
    throw invalid();
  id(value.project_id);
  id(value.revision);
  id(value.sequence_id);
  id(value.idempotency_key);
  if (
    value.project_id !== project ||
    value.sequence_id !== scene ||
    ("language" in value &&
      value.language !== null &&
      (typeof value.language !== "string" ||
        !/^(auto|[a-z]{2,3})$/.test(value.language)))
  )
    throw invalid();
}
function parse(raw: string): Envelope {
  if (new TextEncoder().encode(raw).length > 16384) throw invalid();
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    throw invalid();
  }
  if (!plain(value)) throw invalid();
  keys(value, "entries,schemaVersion");
  if (
    value.schemaVersion !== 1 ||
    !Array.isArray(value.entries) ||
    value.entries.length > 32
  )
    throw invalid();
  const seen = new Set<string>();
  for (const entry of value.entries) {
    if (!plain(entry)) throw invalid();
    identity(entry.identity);
    id(entry.projectId);
    let scope: string;
    if (entry.kind === "caption_recovery") {
      keys(entry, "identity,kind,projectId,recoveryId");
      id(entry.recoveryId);
      scope = JSON.stringify([entry.kind, entry.identity, entry.projectId]);
    } else if (entry.kind === "transcription") {
      keys(entry, "identity,kind,projectId,request,sceneId");
      id(entry.sceneId);
      request(entry.request, entry.projectId, entry.sceneId);
      scope = JSON.stringify([
        entry.kind,
        entry.identity,
        entry.projectId,
        entry.sceneId,
      ]);
    } else throw invalid();
    if (seen.has(scope)) throw invalid();
    seen.add(scope);
  }
  return { schemaVersion: 1, entries: value.entries as Entry[] };
}
function read(): Envelope {
  try {
    const raw = sessionStorage.getItem("opencut.caption-client.v1");
    return raw === null ? { schemaVersion: 1, entries: [] } : parse(raw);
  } catch {
    throw invalid();
  }
}
function write(value: Envelope) {
  const raw = JSON.stringify(value);
  parse(raw);
  try {
    sessionStorage.setItem("opencut.caption-client.v1", raw);
    if (sessionStorage.getItem("opencut.caption-client.v1") !== raw)
      throw invalid();
  } catch {
    throw invalid();
  }
}
function scope(connection: string, project: string, scene?: string) {
  identity(connection);
  id(project);
  if (scene !== undefined) id(scene);
}
/** Only protocol locators and exact ASR public intent are retained. These do not
 * establish authority, project state, a successful edit or a successful job. */
export function readCaptionRecoveryLocator(
  connection: string,
  project: string,
): string | null {
  scope(connection, project);
  const entry = read().entries.find(
    (e) =>
      e.kind === "caption_recovery" &&
      e.identity === connection &&
      e.projectId === project,
  );
  return entry?.kind === "caption_recovery" ? entry.recoveryId : null;
}
export function writeCaptionRecoveryLocator(
  connection: string,
  project: string,
  recoveryId: string,
) {
  scope(connection, project);
  id(recoveryId);
  const value = read();
  const old = value.entries.find(
    (e) =>
      e.kind === "caption_recovery" &&
      e.identity === connection &&
      e.projectId === project,
  );
  if (old && old.kind === "caption_recovery") {
    if (old.recoveryId !== recoveryId) throw invalid();
  } else
    value.entries.push({
      kind: "caption_recovery",
      identity: connection,
      projectId: project,
      recoveryId,
    });
  write(value);
}
export function clearCaptionRecoveryLocator(
  connection: string,
  project: string,
  recoveryId: string,
) {
  scope(connection, project);
  id(recoveryId);
  const value = read();
  const entries = value.entries.filter(
    (e) =>
      !(
        e.kind === "caption_recovery" &&
        e.identity === connection &&
        e.projectId === project &&
        e.recoveryId === recoveryId
      ),
  );
  if (entries.length !== value.entries.length) write({ ...value, entries });
}
export function readTranscriptionRequest(
  connection: string,
  project: string,
  scene: string,
): TranscriptionRequestV4 | null {
  scope(connection, project, scene);
  const entry = read().entries.find(
    (e) =>
      e.kind === "transcription" &&
      e.identity === connection &&
      e.projectId === project &&
      e.sceneId === scene,
  );
  return entry?.kind === "transcription"
    ? structuredClone(entry.request)
    : null;
}
function exactRequest(input: TranscriptionRequestV4) {
  return JSON.stringify([
    input.project_id,
    input.revision,
    input.sequence_id,
    input.idempotency_key,
    Object.hasOwn(input, "language"),
    input.language,
  ]);
}
export function writeTranscriptionRequest(
  connection: string,
  input: TranscriptionRequestV4,
) {
  scope(connection, input.project_id, input.sequence_id);
  request(input, input.project_id, input.sequence_id);
  const value = read();
  const old = value.entries.find(
    (e) =>
      e.kind === "transcription" &&
      e.identity === connection &&
      e.projectId === input.project_id &&
      e.sceneId === input.sequence_id,
  );
  if (
    old?.kind === "transcription" &&
    old.request.idempotency_key === input.idempotency_key &&
    exactRequest(old.request) !== exactRequest(input)
  )
    throw invalid();
  const entries = value.entries.filter(
    (e) =>
      !(
        e.kind === "transcription" &&
        e.identity === connection &&
        e.projectId === input.project_id &&
        e.sceneId === input.sequence_id
      ),
  );
  entries.push({
    kind: "transcription",
    identity: connection,
    projectId: input.project_id,
    sceneId: input.sequence_id,
    request: structuredClone(input),
  });
  write({ ...value, entries });
}
export function clearTranscriptionRequest(
  connection: string,
  input: TranscriptionRequestV4,
) {
  scope(connection, input.project_id, input.sequence_id);
  request(input, input.project_id, input.sequence_id);
  const value = read(),
    entries = value.entries.filter(
      (e) =>
        !(
          e.kind === "transcription" &&
          e.identity === connection &&
          e.projectId === input.project_id &&
          e.sceneId === input.sequence_id &&
          e.request.idempotency_key === input.idempotency_key &&
          exactRequest(e.request) === exactRequest(input)
        ),
    );
  if (entries.length !== value.entries.length) write({ ...value, entries });
}
