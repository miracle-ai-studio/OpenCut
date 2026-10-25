import type {AudioPreviewSession} from "./types";
import type {CachedPicture} from "./picture-cache";
import type {SurfacePictureSource} from "./surface-player";
import type {VisualPreviewRenderer,VisualPreviewExecutionApi} from "./visual-preview-renderer";

type Time=AudioPreviewSession["presentation_time"];
const compare=(a:Time,b:Time)=>BigInt(a.ticks)*BigInt(b.timebase)-BigInt(b.ticks)*BigInt(a.timebase);
/** Execution adapter only: every time/grid, layer and binding comes from Rust. */
export class ScenePictureSource implements SurfacePictureSource {
  private first:CachedPicture|null=null;
  private plan:ReturnType<SurfacePictureSource["descriptor"]>|null=null;
  constructor(private renderer:Pick<VisualPreviewRenderer,"render">&Partial<Pick<VisualPreviewRenderer,"surfacePresenter">>,private api:VisualPreviewExecutionApi,private session:AudioPreviewSession){}
  async initialize(signal:AbortSignal){
    const picture=await this.render(this.session.presentation_time,signal,0);
    try {
      const at=picture.frame.plan.identity.time,next=picture.frame.next_time;
      const delta=compare(next,at);
      if(delta<=0n)throw Error("Invalid native scene frame grid");
      this.plan={project_id:this.session.identity.project_id,revision:this.session.identity.revision,sequence_id:this.session.identity.sequence_id,
        width:picture.pixels.width,height:picture.pixels.height,
        duration:{ticks:this.session.total_frames,timebase:this.session.identity.sample_rate},
        frame_step_seconds:Number(delta)/(next.timebase*at.timebase)};
      signal.throwIfAborted();this.first=picture;
    }catch(error){picture.pixels.close();throw error;}
  }
  get preparationFrames(){return Math.min(24,Math.max(1,Math.floor(.96/this.descriptor().frame_step_seconds)));}
  createPainter(canvas:OffscreenCanvas){return this.renderer.surfacePresenter?.(canvas);}
  descriptor(){if(!this.plan)throw Error("Scene picture source is not initialized");return this.plan;}
  hasFrame(time:Time){return compare(time,{ticks:"0",timebase:1})>=0n&&compare(time,this.descriptor().duration)<0n;}
  async frame(time:Time,signal:AbortSignal,retainedBytes=0):Promise<CachedPicture>{
    signal.throwIfAborted();
    if(this.first){const first=this.first;this.first=null;if(compare(time,first.frame.plan.identity.time)<0n||compare(time,first.frame.next_time)>=0n){first.pixels.close();throw Error("Initial scene picture changed requested interval");}return first;}
    return this.render(time,signal,retainedBytes);
  }
  private async render(time:Time,signal:AbortSignal,retainedBytes:number):Promise<CachedPicture>{
    const {session_id,generation,overlay_epoch,owner_id,lease_id}=this.session;
    const result=await this.renderer.render(this.api,{session_id,generation,overlay_epoch,owner_id,lease_id},signal,undefined,undefined,{time,reservedBytes:retainedBytes,end:{ticks:this.session.total_frames,timebase:this.session.identity.sample_rate},captureSurface:true});
    try {
      signal.throwIfAborted();
      const identity=result.frame.plan.identity;
      if(!result.frame.prepare_ahead||!result.frame.next_time||compare(result.frame.next_time,identity.time)<=0n||compare(time,identity.time)<0n||compare(time,result.frame.next_time)>=0n||
        identity.projectId!==this.session.identity.project_id||identity.revision!==this.session.identity.revision||identity.sequenceId!==this.session.identity.sequence_id||
        (this.plan&&(result.pixels.width!==this.plan.width||result.pixels.height!==this.plan.height)))throw Error("Native scene picture changed its fixed session");
      return result as CachedPicture;
    }catch(error){result.pixels.close();throw error;}
  }
}
