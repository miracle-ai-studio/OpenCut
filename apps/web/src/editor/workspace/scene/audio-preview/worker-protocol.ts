import type { EditorConnection } from "../../../../lib/editor-sdk";
import type { AudioPreviewSession } from "../types";
import type { AudioOutputProgress } from "./output";
import type { DeviceReceipt } from "./device-port";
import type { OutputObservation } from "./output-clock";
export const PUMP_CHANNEL = "opencut-audio-pump";
type Envelope = { channel: typeof PUMP_CHANNEL; token: string };
export type PumpCommand = Envelope &
  (
    | { kind: "init"; connection: EditorConnection }
    | { kind: "start"; session: AudioPreviewSession; port: MessagePort }
    | { kind: "freeze" }
    | { kind: "output"; observation: OutputObservation; stalled: boolean }
    | {kind:"output-clock";port:MessagePort}
    | { kind: "abort" }
    | { kind: "stop"; id: number }
    | { kind: "reportStopped"; id: number; frames: string }
  );
export type PumpEventBody =
  | { kind: "ready" }
  | { kind: "progress"; receipt: DeviceReceipt }
  | {
      kind: "session";
      session: AudioPreviewSession;
      progress: AudioOutputProgress;
    }
  | {
      kind: "reply";
      id: number;
      frames?: string;
      session?: AudioPreviewSession;
      receipt?: DeviceReceipt;
    }
  | { kind: "error"; message: string };

export type PumpEvent = Envelope & PumpEventBody;
