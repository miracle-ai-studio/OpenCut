/** Two readers of the same physical output clock. These are device facts, not
 * project time or a second playback clock. Absolute monotonic timestamps let a
 * worker sample the same affine mapping between main-thread observations. */
export type DeviceTimestamp = {contextTime:number;performanceTime:number;timeOrigin:number};
export type PresentationClockBinding={token:string;sampleRate:number;graphLatencySeconds:number;directPictureCredit?:boolean;outputPort?:MessagePort};
export type PresentationClockSource={bind(port:MessagePort):PresentationClockBinding};
