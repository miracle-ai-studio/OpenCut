import { createEditorClient } from "../../../../lib/editor-sdk";
import { PreviewConsumptionPump } from "./pump";
import { PreviewDevicePort } from "./device-port";
import { PUMP_CHANNEL } from "./worker-protocol";
import type { PumpCommand, PumpEvent, PumpEventBody } from "./worker-protocol";
const scope = self as unknown as {
  onmessage: ((event: MessageEvent<PumpCommand>) => void) | null;
  postMessage(event: PumpEvent): void;
  close(): void;
};
let lastId = 0;
let stoppedFrames: string | null = null;
let token: string | null = null,
  alive = true,
  pump: PreviewConsumptionPump | null = null,
  device: PreviewDevicePort | null = null;
let client: ReturnType<typeof createEditorClient> | null = null;
const send = (payload: PumpEventBody) => {
  if (alive)
    scope.postMessage({
      channel: PUMP_CHANNEL,
      version: 1,
      token: token!,
      ...payload,
    } as PumpEvent);
};
const failure = (error: unknown) => {
  if (!alive) return;
  send({
    kind: "error",
    message:
      error instanceof Error ? error.message : "Scene audio data pump failed.",
  });
  alive = false;
  pump?.abort();
  device?.silence();
};
scope.onmessage = ({ data }) => {
  if (
    data.channel !== PUMP_CHANNEL ||
    data.version !== 1 ||
    !data.token ||
    (token !== null && data.token !== token)
  ) {
    failure(new Error("Invalid audio pump channel."));
    return;
  }
  try {
    if (data.kind === "init") {
      if (token !== null)
        throw new Error("Audio pump connection already captured.");
      token = data.token;
      client = createEditorClient(data.connection);
      send({ kind: "ready" });
      return;
    }
    if (!alive) return;
    if (!client) throw new Error("Audio pump has no captured connection.");
    if (data.kind === "start") {
      if (pump) throw new Error("Audio pump already owns a generation.");
      device = new PreviewDevicePort(
        data.port,
        token!,
        (receipt) => {
          pump!.observe({
            consumedFrames: receipt.consumedFrames,
            stalled: receipt.copied === 0,
            contextTime: 0,
          });
          send({ kind: "progress", receipt });
        },
        failure,
        () => pump!.completed(),
        false,
      );
      pump = new PreviewConsumptionPump(
        {
          open: (session, signal) =>
            client!.openV4AudioPreviewStream(session, { signal }),
          control: (input, signal) =>
            client!.call("opencut_audio_preview_v4_control", input, { signal }),
        },
        device,
        () => alive,
        (session, progress) => send({ kind: "session", session, progress }),
        failure,
      );
      pump.start(data.session);
      return;
    }
    if (data.kind === "freeze") {
      pump?.freeze();
      device?.silence();
      return;
    }
    if (data.kind === "abort") {
      alive = false;
      pump?.abort();
      device?.silence();
      scope.close();
      return;
    }
    if (!pump || !device || !Number.isSafeInteger(data.id) || data.id <= lastId)
      throw new Error("Invalid audio pump request.");
    lastId = data.id;
    if (data.kind === "stop") {
      pump.freeze();
      void device
        .stop()
        .then((frames) => {
          stoppedFrames = frames;
          send({ kind: "reply", id: data.id, frames });
        })
        .catch(failure);
    } else if (data.kind === "reportStopped") {
      if (stoppedFrames === null || data.frames !== stoppedFrames)
        throw new Error("Final device cursor is not confirmed.");
      void pump
        .reportStopped(data.frames)
        .then((session) => send({ kind: "reply", id: data.id, session }))
        .catch(failure);
    }
  } catch (error) {
    failure(error);
  }
};
