export class PreviewSegmentDecoder {
    open(key:string,blob:Blob,width:number,height:number):Promise<unknown>;
    warm(key:string,blob:Blob,width:number,height:number):Promise<void>;
    retain(keys:Set<string>):Promise<void>;
    frame(key:string,blob:Blob,width:number,height:number,at:number,signal:AbortSignal):Promise<ImageBitmap>;
    close():Promise<void>;
}
