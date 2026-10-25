import type { EditorConnection } from "../../../lib/editor-sdk";
import type { VisualPreviewRenderer } from "./visual-preview-renderer";
import type { PictureCachePlan, CachedPicture } from "./picture-cache";
import type {AudioPreviewSession} from "./types";
import type {DeviceTimestamp,PresentationClockBinding} from "./audio-preview/presentation-clock";
import type {SurfaceReport} from "./surface-player";

type Args = Parameters<VisualPreviewRenderer["render"]>;
export type PictureResult = Awaited<ReturnType<VisualPreviewRenderer["render"]>>;
export type PictureRequest = {
  id: number;
  session: Args[1];
  hitQuery: Args[3];
  draft: Omit<NonNullable<Args[4]>, "intentSignal"> | undefined;
  preparation: Args[5];
};
export type PictureCommand = { token: string } & (
  | { kind: "init"; connection: EditorConnection }
  | ({ kind: "render" } & PictureRequest)
  | { kind:"prepareCache";id:number;session:Args[1];time?:PictureCachePlan["duration"];wait?:boolean }
  | {kind:"suspend";id:number}
  | {kind:"stillFrame";id:number;session:Args[1];time:PictureCachePlan["duration"]}
  | { kind:"cachedFrame";id:number;time:PictureCachePlan["duration"] }
  | {kind:"startSurface";id:number;session:AudioPreviewSession;producer?:"scene"|"cache";warmTimes?:PictureCachePlan["duration"][];canvas?:OffscreenCanvas;clockPort:MessagePort;binding:PresentationClockBinding}
  | {kind:"surfaceTimestamp";runId:number;timestamp:DeviceTimestamp}
  | {kind:"stopSurface";runId:number;clear:boolean}
  | { kind: "cancel"; id: number; intent: boolean }
  | { kind: "close" }
);
export type PictureError = {
  name: string; message: string; code?: string; metrics?: object;
  api?: { details: unknown; status: number; requestId: string; warnings: readonly string[] };
};
export type PictureEvent = { token: string } & (
  | { kind: "picture"; id: number; result: PictureResult }
  | { kind:"cachePrepared"; id:number; result:PictureCachePlan }
  | {kind:"suspended";id:number}
  | { kind:"cachedPicture"; id:number; result:CachedPicture }
  | { kind:"stillPicture";id:number;result:PictureResult|CachedPicture }
  | { kind:"cacheProgress"; id:number;completed:number;total:number }
  | {kind:"surfacePrepared";id:number;result:SurfaceReport}
  | {kind:"surfaceReport";runId:number;result:SurfaceReport;retired:boolean}
  | {kind:"surfaceFailed";runId:number;error:PictureError}
  | { kind: "error"; id: number; error: PictureError }
  | { kind: "closed" }
);
