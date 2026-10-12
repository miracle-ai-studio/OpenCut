import type { EditorConnection } from "../../../../lib/editor-sdk";
import type { V4AudioPreviewSession } from "../types";
import type { AudioOutputProgress } from "./output";
import type { DeviceReceipt } from "./device-port";
export const PUMP_CHANNEL = "opencut-audio-pump";
type Envelope = { channel: typeof PUMP_CHANNEL; version: 1; token: string };
export type PumpCommand = Envelope &
  (
    | { kind: "init"; connection: EditorConnection }
    | { kind: "start"; session: V4AudioPreviewSession; port: MessagePort }
    | { kind: "freeze" }
    | { kind: "abort" }
    | { kind: "stop"; id: number }
    | { kind: "reportStopped"; id: number; frames: string }
  );
export type PumpEventBody =
  | { kind: "ready" }
  | { kind: "progress"; receipt: DeviceReceipt }
  | {
      kind: "session";
      session: V4AudioPreviewSession;
      progress: AudioOutputProgress;
    }
  | {
      kind: "reply";
      id: number;
      frames?: string;
      session?: V4AudioPreviewSession;
    }
  | { kind: "error"; message: string };

export type PumpEvent = Envelope & PumpEventBody;
