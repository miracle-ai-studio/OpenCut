import { vi } from "vitest";
import { EditorApi } from "../api";
import { WorkspaceController } from "../controller";
import type { ToolInput, ToolOutput } from "../../../lib/editor-sdk";
import type { Snapshot } from "./types";

/** One final render declaration, shared by existing behavior fixtures. */
export const renderCapability = (): ToolOutput<"opencut_render_capability"> => ({
  frame: {available:true,unavailable_reason:null},
  audio: {available:true,unavailable_reason:null},
  video: {available:true,unavailable_reason:null}, video_audio:true,
  formats:["mp4","webm"], qualities:["low","medium","high","very_high"],
  sample_rates:[44100,48000], submit_tool:"opencut_render_submit",
});

export const initialSnapshot = (): Snapshot => ({
  revision: "r0",
  project: {
    id: "v4-project",
    name: "Complete V4",
    default_sequence_id: "scene",
    assets: [
      {
        id: "asset",
        name: "Image",
        kind: "image",
        origin: { kind: "legacy" },
        version: { id: "version", content_hash: "a".repeat(64), size: 2 },
        metadata: {
          audio: { status: "absent" },
          intrinsic_size: { width: 640, height: 360 },
        },
      },
    ],
    fonts: [],
    settings: {
      profile: "classic_global",
      settings: {
        canvas_size: { width: 640, height: 360 },
        canvas_size_mode: "preset",
        fps: { numerator: 24, denominator: 1 },
        background: {
          kind: "color",
          color: { encoding: "srgb", red: 0, green: 0, blue: 0, alpha: 1 },
        },
      },
    },
    sequences: [
      {
        id: "scene",
        name: "Main Scene",
        main_track_id: "track",
        bookmarks: [],
        captions: [],
        transitions: [],
        tracks: [
          {
            id: "track",
            name: "Main Track",
            kind: "video",
            enabled: true,
            locked: false,
            muted: false,
            opacity: 1,
            gain: { unit: "decibels", value: 0 },
            channels: [],
            elements: [
              {
                id: "image",
                name: "Whole image",
                enabled: true,
                content: { kind: "image", asset_id: "asset" },
                source_span: {
                  profile: "classic_trimmed",
                  trim_start: { ticks: "0", timebase: 1 },
                  trim_end: { ticks: "0", timebase: 1 },
                  source_duration: { ticks: "12", timebase: 1 },
                },
                timeline_range: {
                  start: { ticks: "0", timebase: 1 },
                  duration: { ticks: "12", timebase: 1 },
                },
                channels: [],
                visual: {
                  blend_mode: "normal",
                  profile: "classic_canvas_v31",
                  opacity: 1,
                  effects: [],
                  masks: [],
                  transform: {
                    position_x_px: 17,
                    position_y_px: -20,
                    rotation_degrees: 10,
                    scale_x: 1,
                    scale_y: 1,
                  },
                },
              },
            ],
          },
        ],
      },
    ],
  },
});

/** A scoped protocol fixture, not a substitute for SQLite/browser acceptance. */
export async function setup(snapshot = initialSnapshot()) {
  // These tests exercise commands/ownership, not layout. The real browser
  // verifier owns panel geometry and resizing; jsdom has no ResizeObserver.
  if (typeof window !== "undefined" && !window.ResizeObserver) {
    vi.stubGlobal("ResizeObserver", class {
      observe() {}
      unobserve() {}
      disconnect() {}
    });
  }
  if (typeof sessionStorage === "undefined") {
    const storage = new Map<string, string>();
    vi.stubGlobal("sessionStorage", {
      getItem: (key: string) => storage.get(key) ?? null,
      setItem: (key: string, value: string) => { storage.set(key, value); },
    });
  }
  const api = new EditorApi("http://localhost:8788", "test");
  let current = structuredClone(snapshot);
  const snapshots = new Map([[current.revision, structuredClone(current)]]);
  let before = structuredClone(current),
    redo: Snapshot | null = null;
  let counter = 0;
  const revisions: ToolOutput<"opencut_project_history">["revisions"] = [
    {
      revision: "r0",
      created_at: "2026-10-12T00:00:00Z",
      label: "Create project",
    },
  ];
  const ref = () => ({
    id: current.project.id,
    name: current.project.name,
    revision: current.revision,
    format: "editor" as const,
  });
  vi.spyOn(api.sdk, "hostCapabilities").mockResolvedValue({ tools: [] } as never);
  vi.spyOn(api, "projects").mockImplementation(async () => [ref()]);
  vi.spyOn(api, "tasks").mockResolvedValue({ jobs: [], next_cursor: null });
  const publish = (candidate: Snapshot, label: string) => {
    current = structuredClone(candidate);
    current.revision = `r${++counter}`;
    snapshots.set(current.revision, structuredClone(current));
    revisions.unshift({
      revision: current.revision,
      parent_revision: revisions[0].revision,
      created_at: "2026-10-12T01:00:00Z",
      transaction_id: `tx${counter}`,
      label,
    });
  };
  type RecoveryRecord = NonNullable<ToolOutput<"opencut_edit_recovery_inspect">["record"]>;
  let recovery: RecoveryRecord | null = null;
  const call = vi.spyOn(api.sdk, "call");
  call.mockImplementation(async (name, input) => {
      if (name === "opencut_render_capability") return renderCapability() as never;
      if (name === "opencut_access_inspect")
        return { principal_id: "fixture-principal", credential_id: "fixture-credential", owner: false, grants: [] } as never;
      if (name === "opencut_project_inspect") {
        const request = input as ToolInput<"opencut_project_inspect">;
        return {kind:"editor",snapshot:structuredClone(
          request.revision ? snapshots.get(request.revision)! : current,
        )} as never;
      }
      if (name === "opencut_edit_recovery_stage") {
        const context = (input as ToolInput<"opencut_edit_recovery_stage">).context;
        if (recovery && JSON.stringify(recovery.context) !== JSON.stringify(context)) throw new Error("Fixture unresolved original edit");
        recovery ??= { recovery_id: crypto.randomUUID(), project_id: context.intent.request.project_id,
          request_digest: "fixture-opaque-context-digest", context: structuredClone(context), receipt: null };
        return structuredClone(recovery) as never;
      }
      if (name === "opencut_edit_recovery_inspect") return { record: recovery ? structuredClone(recovery) : null } as never;
      if (name === "opencut_edit_recovery_ack") {
        if (!recovery) throw new Error("Fixture has no staged recovery");
        // This test transport records the already-returned public reply; it
        // does not reproduce SQL authorization/head/receipt proof rules.
        const intent = recovery.context.intent;
        const op = intent.kind === "public_apply" ? "opencut_edit_apply" : intent.kind === "undo" ? "opencut_edit_undo_apply"
          : intent.kind === "redo" ? "opencut_edit_redo_apply" : intent.kind === "restore" ? "opencut_project_restore" : intent.kind === "subtitle_generate" ? "opencut_subtitle_generate" : intent.kind==="font_import" ? "opencut_font_import" : "opencut_subtitle_import";
        let index = -1;
        call.mock.calls.forEach(([name, value], i) => { if (name === op && (value as { idempotency_key?: string }).idempotency_key === intent.request.idempotency_key) index = i; });
        if (index < 0) throw new Error("Fixture has no returned original receipt");
        const result = await call.mock.results[index].value;
        const receipt = intent.kind === "public_apply" ? { kind: "public_apply", transaction: result } : intent.kind === "restore" ? { kind: "restore", transaction: result }
          : intent.kind === "subtitle_generate" || intent.kind === "subtitle_import" ? { kind: intent.kind, caption: result } : intent.kind==="font_import" ? {kind:"font_import",transaction:result} : { kind: intent.kind, commit: result };
        const id = recovery.recovery_id; recovery = null;
        return { recovery_id: id, resolution: "saved", receipt } as never;
      }
      if (name === "opencut_project_media_query")
        return { media: [], next_cursor: null } as never;
      if (name === "opencut_project_history")
        return {
          project_id: current.project.id,
          head_revision: current.revision,
          revisions: structuredClone(revisions),
          next_cursor: null,
        } as never;
      if (name === "opencut_project_history_navigation")
        return {
          project_id: current.project.id,
          revision: current.revision,
          undo: counter
            ? { transaction_id: `tx${counter}`, label: "Bookmark edit" }
            : null,
          redo: redo
            ? { transaction_id: "tx1", undo_transaction_id: `tx${counter}` }
            : null,
        } as never;
      if (name === "opencut_edit_plan")
        return { project: structuredClone(current.project) } as never;
      if (name === "opencut_edit_apply") {
        const request = input as ToolInput<"opencut_edit_apply">;
        if (request.expected_revision !== current.revision)
          throw new Error("Revision conflict");
        before = structuredClone(current);
        const next = structuredClone(current);
        for (const op of request.operations) {
          if (op.type !== "bookmark")
            throw new Error("Unexpected non-bookmark operation");
          const action = op.operation,
            s = next.project.sequences[0];
          if (action.kind === "put")
            s.bookmarks = [
              ...s.bookmarks.filter((b) => b.id !== action.bookmark.id),
              action.bookmark,
            ];
          if (action.kind === "move") {
            const b = s.bookmarks.find((b) => b.id === action.bookmark_id)!;
            b.time = action.time;
          }
          if (action.kind === "delete")
            s.bookmarks = s.bookmarks.filter(
              (b) => b.id !== action.bookmark_id,
            );
        }
        publish(next, request.label ?? "Bookmark edit");
        return { base_revision: request.expected_revision, revision: current.revision, transaction_id: `tx${counter}` } as never;
      }
      if (
        name === "opencut_edit_undo_plan" ||
        name === "opencut_edit_redo_plan"
      )
        return {
          direction: name.includes("undo") ? "undo" : "redo",
          expected_revision: current.revision,
          target_transaction_id: "tx1",
          changes: [{ path: "sequences.scene.bookmarks" }],
          conflicts: [],
        } as never;
      if (name === "opencut_edit_undo_apply") {
        const request = input as ToolInput<"opencut_edit_undo_apply">;
        redo = structuredClone(current);
        publish(before, "Undo bookmark");
        return { direction: "undo", target_transaction_id: request.target_transaction_id, undo_transaction_id: null,
          transaction: { base_revision: request.expected_revision, revision: current.revision, transaction_id: `tx${counter}` } } as never;
      }
      if (name === "opencut_edit_redo_apply") {
        const request = input as ToolInput<"opencut_edit_redo_apply">;
        publish(redo!, "Redo bookmark");
        redo = null;
        return { direction: "redo", target_transaction_id: "tx1", undo_transaction_id: request.undo_transaction_id,
          transaction: { base_revision: request.expected_revision, revision: current.revision, transaction_id: `tx${counter}` } } as never;
      }
      if (name === "opencut_project_restore") {
        const request = input as ToolInput<"opencut_project_restore">;
        publish(snapshots.get(request.target_revision)!, "Restore project revision");
        return { base_revision: request.expected_revision, revision: current.revision, transaction_id: `tx${counter}` } as never;
      }
      throw new Error(`Unexpected tool ${name}`);
    });
  const controller = new WorkspaceController(() => api);
  await controller.connect("local", "token");
  await controller.open(ref());
  return { api, controller, call, current: () => current, ref, snapshots };
}
