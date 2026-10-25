// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor, act } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { WorkspaceProvider } from "../context";
import { setupAudioWav } from "./audio-wav-fixture";
import { ExportRecords } from "./export-records";
import { ArtifactDownload } from "./artifact-download";
import { exportRecord, isUserExport, publishedArtifacts } from "./export-record";
import type { Job, ArtifactReference } from "../../../lib/editor-sdk";
afterEach(()=>{cleanup();vi.restoreAllMocks();delete (window as Window & {showSaveFilePicker?:unknown}).showSaveFilePicker;});
const artifact:ArtifactReference={artifact_id:"artifact_"+"a".repeat(32),job_id:"wav",content_hash:"b".repeat(64),size_bytes:"3",mime_type:"audio/wav",role:"primary",availability:"unchecked"};
const job=():Job=>({job_id:"wav",kind:"render.audio",state:"succeeded",cancel_requested:false,target:{project_id:"v4-project",revision:"old-revision",sequence_id:"scene"},artifacts:[artifact],failure:null,result_unavailable:null,result:{kind:"render.audio",data:{identity:{frame_count:"96000",sample_rate:48000,channels:2}} as never}});
it("keeps internal previews separate and does not expose downloads for unpublished or unavailable results",()=>{
 const row=job();expect(isUserExport(row)).toBe(true);expect(isUserExport({...row,purpose:"playback_optimization"})).toBe(false);expect(isUserExport({...row,kind:"render.frame"})).toBe(false);
 expect(publishedArtifacts(row)).toEqual([artifact]);
 for(const patch of [{state:"running"},{state:"failed"},{result:null},{failure:{code:"FAILED",message:"Failed",diagnostic_job_id:"wav"}},{result_unavailable:{code:"NOT_FOUND",message:"File unavailable",diagnostic_job_id:"wav"}},{artifacts:[{...artifact,job_id:"another-job"}]}])expect(publishedArtifacts({...row,...patch} as Job)).toEqual([]);
 expect(exportRecord(row)).toEqual({format:"WAV",size:null,fps:null,seconds:2,rate:48000,channels:2});
 expect(exportRecord({...row,result:null})).toEqual({format:null,size:null,fps:null,seconds:null,rate:null,channels:null});
});
it("shows only exports, searches real receipt fields and changes filters without writes",async()=>{
 const f=await setupAudioWav();try{
  const done=job(),failed={...job(),job_id:"failed-export",state:"failed" as const,artifacts:[],result:null,failure:{code:"DISK",message:"Storage capacity low",diagnostic_job_id:"failed-export"}};
  f.rows.push(done,failed,{...job(),job_id:"internal",purpose:"playback_optimization"},{...job(),job_id:"frame",kind:"render.frame"});await f.controller.sync();
  render(<WorkspaceProvider controller={f.controller}><ExportRecords onExport={vi.fn()}/></WorkspaceProvider>);
  expect(screen.getAllByRole("button",{name:/^Export details/})).toHaveLength(2);expect(screen.queryByText("internal")).toBeNull();expect(screen.queryByRole("button",{name:"Download"})).toBeNull();
  fireEvent.click(screen.getByRole("button",{name:"Failed"}));expect(screen.getAllByRole("button",{name:/^Export details/})).toHaveLength(1);
  fireEvent.click(screen.getByRole("button",{name:/^Export details/}));expect(screen.getByText("failed-export")).toBeTruthy();expect(screen.getAllByText("—").length).toBeGreaterThan(0);
  fireEvent.click(screen.getByRole("button",{name:"All exports"}));fireEvent.change(screen.getByRole("searchbox"),{target:{value:"failed-export"}});expect(screen.getAllByRole("button",{name:/^Export details/})).toHaveLength(1);
  expect(f.requests).toHaveLength(0);expect(f.rangeRequests).toHaveLength(0);
 }finally{f.controller.dispose();}
});
it("checks published bytes on intent, releases the check stream, and reauthorizes on download",async()=>{
 const f=await setupAudioWav();try{
  const cancel=vi.fn(),download=vi.spyOn(f.api.sdk,"downloadArtifact").mockResolvedValueOnce(new Response(new ReadableStream({cancel}),{status:200})).mockResolvedValueOnce(new Response(new Uint8Array([1,2,3])));
  const saved:number[][]=[];Object.defineProperty(window,"showSaveFilePicker",{configurable:true,value:vi.fn().mockResolvedValue({createWritable:async()=>new WritableStream<Uint8Array>({write(c){saved.push([...c]);}})})});
  render(<WorkspaceProvider controller={f.controller}><ArtifactDownload artifact={artifact} name="existing.wav"/></WorkspaceProvider>);
  expect(download).not.toHaveBeenCalled();expect(screen.queryByRole("button",{name:"Download"})).toBeNull();
  fireEvent.click(screen.getByRole("button",{name:"Check file"}));await screen.findByRole("button",{name:"Download"});expect(cancel).toHaveBeenCalledOnce();
  fireEvent.click(screen.getByRole("button",{name:"Download"}));await waitFor(()=>expect(saved).toEqual([[1,2,3]]));expect(download).toHaveBeenCalledTimes(2);
 }finally{f.controller.dispose();}
});
it("keeps a missing file undownloadable and discards a late check after disconnect",async()=>{
 const f=await setupAudioWav();try{
  const download=vi.spyOn(f.api.sdk,"downloadArtifact").mockRejectedValueOnce(Error("Published artifact not found"));
  const view=render(<WorkspaceProvider controller={f.controller}><ArtifactDownload artifact={artifact} name="existing.wav"/></WorkspaceProvider>);
  fireEvent.click(screen.getByRole("button",{name:"Check file"}));await screen.findByRole("button",{name:"Check again"});expect(screen.queryByRole("button",{name:"Download"})).toBeNull();
  let resolve!:(r:Response)=>void;download.mockImplementationOnce(()=>new Promise(done=>{resolve=done;}));fireEvent.click(screen.getByRole("button",{name:"Check again"}));await waitFor(()=>expect(download).toHaveBeenCalledTimes(2));
  const signal=download.mock.calls[1][1]!.signal;view.unmount();expect(signal?.aborted).toBe(true);const cancelled=vi.fn();await act(async()=>{resolve(new Response(new ReadableStream({cancel:cancelled})));});expect(cancelled).toHaveBeenCalledOnce();
 }finally{f.controller.dispose();}
});
it("cancels only the chosen active export through the durable task port",async()=>{
 const f=await setupAudioWav();try{
  const row={...job(),state:"running" as const,result:null,artifacts:[]};f.rows.push(row);await f.controller.sync();const cancel=vi.spyOn(f.api.sdk,"cancelJob").mockImplementation(async()=>{row.cancel_requested=true;return row;});
  render(<WorkspaceProvider controller={f.controller}><ExportRecords onExport={vi.fn()}/></WorkspaceProvider>);fireEvent.click(screen.getByRole("button",{name:"Cancel export"}));
  await waitFor(()=>expect(cancel).toHaveBeenCalledWith("wav"));expect((await screen.findByRole("button",{name:"Cancelling…"}) as HTMLButtonElement).disabled).toBe(true);expect(f.requests).toHaveLength(0);
 }finally{f.controller.dispose();}
});
