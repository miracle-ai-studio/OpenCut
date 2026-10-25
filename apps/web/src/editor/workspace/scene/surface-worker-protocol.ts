import type {AudioPreviewSession} from './types';
import type {SurfacePictureSource,SurfaceReport} from './surface-player';
import type {CachedPicture} from './picture-cache';
import type {DeviceTimestamp,PresentationClockBinding} from './audio-preview/presentation-clock';
export type SurfaceDescriptor=ReturnType<SurfacePictureSource['descriptor']>;
export const PRODUCER_AHEAD=6;
export type SurfaceWorkerError={name:string;message:string;code?:string};
export const surfaceError=(error:unknown):SurfaceWorkerError=>({name:error instanceof Error?error.name:'Error',message:error instanceof Error?error.message:String(error),...(error instanceof Error&&'code' in error?{code:String(error.code)}:{})});
export type SurfaceCommand={token:string}&(
 |{kind:'start';canvas?:OffscreenCanvas;session:AudioPreviewSession;plan:SurfaceDescriptor;capacity:number;clockPort:MessagePort;binding:PresentationClockBinding}
 |{kind:'picture';id:number;picture:CachedPicture}|{kind:'pictureError';id:number;error:SurfaceWorkerError}
 |{kind:'timestamp';timestamp:DeviceTimestamp}|{kind:'stop';clear:boolean}|{kind:'close'});
export type SurfaceEvent={token:string}&(
 |{kind:'frame';id:number;time:AudioPreviewSession['presentation_time'];retainedBytes:number}
 |{kind:'report'|'started'|'stopped';report:SurfaceReport}
 |{kind:'failed';error:SurfaceWorkerError});
