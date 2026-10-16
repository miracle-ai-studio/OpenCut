import { useWorkspace } from "../workspace/context";
import type { WorkspaceController } from "../workspace/controller";
import type { V4Operation, V4Sequence } from "../workspace/v4/types";
import { fontRecoveryBlocked } from "../workspace/v4/text/font-recovery";
import { sourceAudioAvailability } from "../workspace/v4/source-audio";
import { audioGainAvailability } from "../workspace/v4/audio-gain";
import { addTime, compareTime, zero } from "../model/time";
import type { Time } from "../model/types";
import { useShortcutActions } from "./provider";
import type { ShortcutHandlers } from "./registry";

/** Exact navigation only; media clock mapping and edit planning remain in the
 * existing V4 controllers/application. No secondary player or document model. */
export function shortcutSequenceEnd(sequence: V4Sequence): Time {
  const ranges = [
    ...sequence.tracks.flatMap((track) =>
      track.elements.map((element) => element.timeline_range),
    ),
    ...sequence.captions.map((caption) => caption.timeline_range),
  ];
  return ranges.reduce((end, range) => {
    const candidate = addTime(range.start, range.duration);
    return compareTime(candidate, end) > 0 ? candidate : end;
  }, zero);
}

export function v4ShortcutActions(
  controller: WorkspaceController,
): ShortcutHandlers {
  const workspace = controller.getSnapshot(),
    state = workspace.v4;
  if (!workspace.api || !state) return {};
  const project = (state.historical ?? state.head).project;
  const sequence = project.sequences.find(
    (item) => item.id === state.sequenceId,
  );
  if (!sequence) return {};
  const session = controller.v4;
  const identity = session.timelineEdit.identity();
  const pending =
    workspace.busy ||
    !!state.recovery?.record ||
    !!state.recovery?.error ||
    fontRecoveryBlocked(state.fontRecovery) ||
    session.captionRecovery.blocked() ||
    !!state.timelineEdit.attempt ||
    !!state.clipboard.attempt ||
    !!state.timelineEdit.recoveryError ||
    !!state.gainDraft ||
    (!!state.audioGain && state.audioGain.phase !== "saved") ||
    (!!state.sourceAudio &&
      (state.sourceAudio.phase !== "saved" ||
        !state.sourceAudio.headChecked)) ||
    (!!state.media.attempt && state.media.attempt.phase !== "saved");
  const editReason = state.historical
    ? "Return to the current revision before editing."
    : pending
      ? "Finish or recover the current edit first."
      : null;
  const navigationReason =
    !!state.gainDraft ||
    workspace.busy ||
    state.audioPreview.phase === "unconfirmed"
      ? "Finish the current interaction first."
      : null;
  const end = shortcutSequenceEnd(sequence);
  const format =
    project.settings.profile === "classic_global"
      ? project.settings.settings
      : project.settings.formats.find(
          (item) => item.sequence_id === sequence.id,
        );
  const selected = sequence.tracks.flatMap((track) =>
    track.elements
      .filter(
        (element) =>
          state.timelineEdit.selection.some(
            (target) =>
              target.track_id === track.id && target.element_id === element.id,
          ) ||
          (!state.timelineEdit.selection.length &&
            state.selectedId === element.id),
      )
      .map((element) => ({ track, element })),
  );
  const selectionReason =
    !selected.length || selected.some((item) => item.track.locked)
      ? "Select elements on unlocked tracks."
      : null;
  const visual = selected.filter((item) =>
    ["video", "image", "text", "graphic"].includes(item.element.content.kind),
  );
  const audible = selected.filter(
    (item) =>
      item.element.content.kind === "audio" ||
      item.element.content.kind === "video",
  );
  const audioReason =
    !audible.length ||
    audible.some(
      (item) => audioGainAvailability(state, item.element.id)?.reason,
    )
      ? "Select audio or video elements with known audio."
      : null;
  const source = sourceAudioAvailability(state);
  const playing = ["playing", "stalled", "loading"].includes(
    state.audioPreview.phase,
  );
  const handler = (
    run: () => unknown,
    reason: string | null,
    checked?: boolean,
  ) => ({ run, enabled: !reason, reason: reason ?? undefined, checked });
  const current = () => {
    const next = controller.getSnapshot();
    const latestIdentity = session.timelineEdit.identity();
    if (
      next.api !== workspace.api ||
      next.v4?.head.project.id !== state.head.project.id ||
      next.v4.sequenceId !== sequence.id ||
      latestIdentity.api !== identity.api ||
      latestIdentity.generation !== identity.generation ||
      latestIdentity.viewGeneration !== identity.viewGeneration ||
      next.v4.selectedId !== state.selectedId ||
      JSON.stringify(next.v4.timelineEdit.selection) !==
        JSON.stringify(state.timelineEdit.selection) ||
      (next.v4.historical ?? next.v4.head).revision !==
        (state.historical ?? state.head).revision
    )
      throw new Error("The editor view changed before the action could run.");
    return next.v4;
  };
  const seek = (delta: Time | "start" | "end") => {
    const live = current();
    const candidate =
      delta === "start"
        ? zero
        : delta === "end"
          ? end
          : addTime(live.playhead, delta);
    session.seek(
      compareTime(candidate, zero) < 0
        ? zero
        : compareTime(candidate, end) > 0
          ? end
          : candidate,
    );
  };
  const writeElements = (operations: V4Operation[], label: string) => {
    current();
    return session.write(label, operations);
  };
  const actions: ShortcutHandlers = {
    "toggle-play": handler(
      () => {
        const live = current();
        return ["playing", "stalled", "loading"].includes(
          live.audioPreview.phase,
        )
          ? session.audioPreview.pause()
          : session.audioPreview.play(undefined, session.visualPreview.start());
      },
      state.audioPreview.phase === "unconfirmed" ||
        !state.audioPreview.capability?.available
        ? "Live scene preview is unavailable."
        : workspace.busy
          ? "Finish the current interaction first."
          : null,
    ),
    "stop-playback": handler(async () => {
      current();
      await session.audioPreview.pause();
      current();
      session.seek(zero);
    }, navigationReason),
    "seek-forward": handler(
      () => seek({ ticks: "1", timebase: 1 }),
      navigationReason,
    ),
    "seek-backward": handler(
      () => seek({ ticks: "-1", timebase: 1 }),
      navigationReason,
    ),
    "jump-forward": handler(
      () => seek({ ticks: "5", timebase: 1 }),
      navigationReason,
    ),
    "jump-backward": handler(
      () => seek({ ticks: "-5", timebase: 1 }),
      navigationReason,
    ),
    "frame-step-forward": handler(
      () =>
        seek({
          ticks: String(format!.fps.denominator),
          timebase: format!.fps.numerator,
        }),
      navigationReason ??
        (!format ? "This action is currently unavailable." : null),
    ),
    "frame-step-backward": handler(
      () =>
        seek({
          ticks: String(-format!.fps.denominator),
          timebase: format!.fps.numerator,
        }),
      navigationReason ??
        (!format ? "This action is currently unavailable." : null),
    ),
    "goto-start": handler(() => seek("start"), navigationReason),
    "goto-end": handler(() => seek("end"), navigationReason),
    "select-all": handler(
      () => {
        current();
        session.timelineEdit.selectMany(
          sequence.tracks.flatMap((track) =>
            track.elements.map((element) => ({
              track_id: track.id,
              element_id: element.id,
            })),
          ),
        );
      },
      pending ? "Finish the current interaction first." : null,
    ),
    "deselect-all": handler(
      () => {
        current();
        session.select(null);
      },
      pending ? "Finish the current interaction first." : null,
    ),
    "cancel-interaction": handler(
      () => {
        current();
        session.audioGain.cancel();
        session.select(null);
      },
      workspace.busy ? "Finish the current interaction first." : null,
    ),
    undo: handler(
      () => {
        current();
        return session.planUndo(state.navigation.undo!.transaction_id);
      },
      editReason ?? (!state.navigation.undo ? "No undo is available." : null),
    ),
    redo: handler(
      () => {
        current();
        return session.planRedo(state.navigation.redo!.undo_transaction_id);
      },
      editReason ?? (!state.navigation.redo ? "No redo is available." : null),
    ),
    "toggle-bookmark": handler(() => {
      const live = current();
      return session.bookmark({
        kind: "toggle",
        sequence_id: sequence.id,
        time: live.playhead,
        bookmark_id: crypto.randomUUID(),
      });
    }, editReason),
    "toggle-source-audio": handler(
      () => {
        current();
        return session.sourceAudio.start(
          source!.sourceEnabled ? "separate" : "restore",
        );
      },
      editReason ??
        source?.reason ??
        (!source ? "Select a video element to edit its source audio." : null),
    ),
    "toggle-elements-visibility-selected": handler(
      () => {
        const enabled = !visual.some((item) => item.element.enabled);
        return writeElements(
          visual.map(({ track, element }) => ({
            type: "element_put",
            sequence_id: sequence.id,
            track_id: track.id,
            element: { ...element, enabled },
          })),
          "Toggle element visibility",
        );
      },
      editReason ??
        selectionReason ??
        (!visual.length
          ? "Select visual elements to change visibility."
          : null),
    ),
    "toggle-elements-muted-selected": handler(
      () => {
        current();
        const mute = audible.some(
          ({ element }) =>
            (element.content.kind === "audio" ||
              element.content.kind === "video") &&
            !element.content.audio.muted,
        );
        if (audible.length === 1)
          return session.audioGain.mute(audible[0].element.id, mute);
        // Multi-selection is one public atomic transaction, preserving all other
        // source/audio fields. Single-element live audio retains its overlay port.
        const operations: V4Operation[] = audible.map(({ track, element }) => {
          const content = element.content;
          if (content.kind !== "audio" && content.kind !== "video")
            throw new Error("Select audio or video elements with known audio.");
          return {
            type: "element_put",
            sequence_id: sequence.id,
            track_id: track.id,
            element: {
              ...element,
              content: { ...content, audio: { ...content.audio, muted: mute } },
            },
          };
        });
        return writeElements(operations, "Toggle element mute");
      },
      editReason ??
        selectionReason ??
        audioReason ??
        (audible.length > 1 && playing
          ? "Pause playback before editing multiple audio elements."
          : null),
    ),
  };
  for (const [id, action] of Object.entries(actions)) {
    const run = action.run;
    action.run = () => {
      current();
      // Affordances may have changed since React last committed. Re-read them
      // at the action boundary; application authorization remains authoritative.
      const live = v4ShortcutActions(controller)[id as keyof ShortcutHandlers];
      if (!live || live.enabled === false)
        throw new Error(
          live?.reason ?? "This action is currently unavailable.",
        );
      return run();
    };
  }
  return actions;
}

export function V4WorkspaceShortcutActions({
  active = true,
}: {
  active?: boolean;
}) {
  const { controller } = useWorkspace();
  useShortcutActions(active ? v4ShortcutActions(controller) : {}, {
    priority: -10,
  });
  return null;
}
