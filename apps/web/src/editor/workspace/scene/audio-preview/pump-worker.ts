import { createEditorClient } from "../../../../lib/editor-sdk";
import { PreviewConsumptionPump } from "./pump";
import { PreviewDevicePort } from "./device-port";
import { PUMP_CHANNEL } from "./worker-protocol";
import type { PumpCommand, PumpEvent, PumpEventBody } from "./worker-protocol";
import {counter} from './session-identity';
import type {OutputObservation} from './output-clock';
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
let outputPort:MessagePort|null=null;
let outputCaptured=false;
const send = (payload: PumpEventBody) => {
  if (alive)
    scope.postMessage({
      channel: PUMP_CHANNEL,
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
  outputPort?.close();outputPort=null;
};
scope.onmessage = ({ data }) => {
  if (
    data.channel !== PUMP_CHANNEL ||
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
    if(data.kind==='output-clock'){
      if(outputCaptured)throw Error('Device output clock already captured');outputCaptured=true;outputPort=data.port;
      data.port.onmessage=({data:receipt}:MessageEvent<{token:string;kind:'output'|'retired';observation?:OutputObservation;stalled?:boolean}>)=>{
        if(!alive||!receipt||receipt.token!==token)return;
        if(receipt.kind==='retired'){outputPort?.close();outputPort=null;return;}
        const output=receipt.observation;
        if(receipt.kind!=='output'||!output||!counter(output.frames)||output.source!=='device_timestamp'||output.uncertainty_frames!==null||typeof receipt.stalled!=='boolean'){failure(Error('Invalid device output clock receipt'));return;}
        try{pump?.observeOutput(output,receipt.stalled);}catch(error){failure(error);}
      };data.port.start();return;
    }
    if (data.kind === "start") {
      if (pump) throw new Error("Audio pump already owns a generation.");
      device = new PreviewDevicePort(
        data.port,
        token!,
        (receipt) => {
          pump!.observe({
            consumedFrames: receipt.consumedFrames,
            stalled: receipt.phase === "buffering" || receipt.phase === "preparing",
            contextTime: 0,
          });
          // Submission credit stays in this owner. The same Worklet receipt
          // reaches the output clock directly through its dedicated port;
          // forwarding it through this worker adds latency and scheduling work
          // to the PCM read/credit loop. Stop still returns the validated tail.
        },
        failure,
        () => pump!.completed(),
        false,
      );
      pump = new PreviewConsumptionPump(
        {
          open: (session, signal) =>
            client!.openAudioPreviewStream(session, { signal }),
          control: (input, signal) =>
            client!.call("opencut_audio_preview_control", input, { signal }),
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
    if (data.kind === "output") {
      if(!outputPort)pump?.observeOutput(data.observation, data.stalled);
      return;
    }
    if (data.kind === "abort") {
      alive = false;
      pump?.abort();
      device?.silence();
      outputPort?.close();outputPort=null;
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
          send({ kind: "reply", id: data.id, frames, receipt: device!.lastReceipt ?? undefined });
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
