// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor, within, act } from "@testing-library/react";
import { afterEach, beforeAll, expect, it, vi } from "vitest";
import { WorkspaceProvider } from "../context";
import { History } from "./history";
import { setup } from "./fixture";
import type { Snapshot } from "./types";
import type { ToolInput, ToolOutput } from "../../../lib/editor-sdk";
beforeAll(()=>{HTMLDialogElement.prototype.showModal=function(){this.setAttribute("open","");}; HTMLDialogElement.prototype.close=function(){this.removeAttribute("open");};});
afterEach(()=>{cleanup();vi.restoreAllMocks();});
function openVersionActions(index=0) { fireEvent.click(screen.getAllByRole("button",{name:/^Version actions/})[index]); return screen.getByRole("group",{name:"Version actions"}); }
it("keeps version actions on demand, closes them with Escape or outside pointer, and restores focus", async () => {
 const f=await setup(); try {
  const inspect=vi.spyOn(f.api.scene,"inspect");
  render(<WorkspaceProvider controller={f.controller}><History/></WorkspaceProvider>);
  expect(screen.queryByRole("button",{name:"View revision"})).toBeNull();
  const more=screen.getByRole("button",{name:/^Version actions/});
  const group=openVersionActions();
  expect(within(group).getByRole<HTMLButtonElement>("button",{name:"Restore this revision"}).disabled).toBe(true);
  fireEvent.keyDown(within(group).getByRole("button",{name:"View revision"}),{key:"Escape"});
  expect(screen.queryByRole("group",{name:"Version actions"})).toBeNull(); expect(document.activeElement).toBe(more);
  openVersionActions(); fireEvent.pointerDown(document.body);
  expect(screen.queryByRole("group",{name:"Version actions"})).toBeNull(); expect(inspect).not.toHaveBeenCalled();
  const summary=screen.getByRole("button",{name:/^View changes/}); fireEvent.click(summary);
  await screen.findByRole("list",{name:"Changes"}); fireEvent.keyDown(summary,{key:"Escape"});
  expect(screen.queryByRole("list",{name:"Changes"})).toBeNull(); expect(document.activeElement).toBe(summary);
 } finally {f.controller.dispose();}
});

it("retains the causal operation when a saved comparison supplies a specific object summary", async () => {
 const f=await setup(); try {
  await f.controller.scene.bookmark({kind:"put",bookmark:{id:"marker",sequence_id:"scene",time:{ticks:"1",timebase:1},note:"Opening"}});
  f.controller.getSnapshot().scene!.history.revisions[0].label="Undo change";
  render(<WorkspaceProvider controller={f.controller}><History/></WorkspaceProvider>);
  fireEvent.click(screen.getByRole("button",{name:"View changes · Undo change"}));
  await screen.findByRole("button",{name:/^Hide changes · Undo change · Added Marker/});
  fireEvent.click(screen.getByRole("button",{name:/^Hide changes · Undo change/}));
  expect(screen.getByRole("button",{name:/^View changes · Undo change · Added Marker/})).toBeTruthy();
 } finally {f.controller.dispose();}
});

it("does not infer project creation from an unlabeled non-root revision", async () => {
 const f=await setup(); try {
  await f.controller.scene.bookmark({kind:"put",bookmark:{id:"marker",sequence_id:"scene",time:{ticks:"1",timebase:1}}});
  const state=f.controller.getSnapshot().scene!; state.history.revisions[0].label=null; state.history.revisions[1].label=null; state.history.revisions[1].parent_revision=null;
  render(<WorkspaceProvider controller={f.controller}><History/></WorkspaceProvider>);
  expect(screen.getByRole("button",{name:"View changes · Project edited"})).toBeTruthy();
  expect(screen.getByRole("button",{name:"View changes · Project created"})).toBeTruthy();
 } finally {f.controller.dispose();}
});
it("ordinary undo and redo each save once without a separate apply click", async()=>{
 const f=await setup(); try {
  await f.controller.scene.bookmark({kind:"put",bookmark:{id:"marker",sequence_id:"scene",time:{ticks:"1",timebase:1},note:"A"}});
  render(<WorkspaceProvider controller={f.controller}><History/></WorkspaceProvider>);
  fireEvent.click(screen.getByRole("button",{name:"Undo"})); await waitFor(()=>expect(f.current().project.sequences[0].bookmarks).toHaveLength(0));
  await waitFor(()=>expect((screen.getByRole("button",{name:"Redo"}) as HTMLButtonElement).disabled).toBe(false));
  fireEvent.click(screen.getByRole("button",{name:"Redo"})); await waitFor(()=>expect(f.current().project.sequences[0].bookmarks).toHaveLength(1));
  expect(f.call.mock.calls.filter(([name])=>name==="opencut_edit_undo_apply")).toHaveLength(1); expect(f.call.mock.calls.filter(([name])=>name==="opencut_edit_redo_apply")).toHaveLength(1);
 } finally {f.controller.dispose();}
});
it("restore requires confirmation and refuses a head that changed while the confirmation was open",async()=>{
 const f=await setup();try{
  await f.controller.scene.bookmark({kind:"put",bookmark:{id:"marker",sequence_id:"scene",time:{ticks:"1",timebase:1}}});
  render(<WorkspaceProvider controller={f.controller}><History/></WorkspaceProvider>);
  fireEvent.click(within(openVersionActions(1)).getByRole("button",{name:"Restore this revision"}));
  let dialog=screen.getByRole("dialog",{name:"Restore this revision"}); expect(f.call.mock.calls.filter(([name])=>name==="opencut_project_restore")).toHaveLength(0);
  fireEvent.click(within(dialog).getByRole("button",{name:"Cancel"})); expect(screen.queryByRole("dialog")).toBeNull();
  fireEvent.click(within(openVersionActions(1)).getByRole("button",{name:"Restore this revision"}));
  await act(async()=>{await f.controller.scene.bookmark({kind:"delete",sequence_id:"scene",bookmark_id:"marker"});});
  dialog=screen.getByRole("dialog",{name:"Restore this revision"});
  expect(within(dialog).getByText("The project changed. Close this window and review the restore again.")).toBeTruthy();
  expect(within(dialog).getByRole<HTMLButtonElement>("button",{name:"Restore project"}).disabled).toBe(true);
  await expect(f.controller.scene.restoreRevision("r0", "r1")).rejects.toThrow(/project changed/i);
  expect(f.call.mock.calls.filter(([name])=>name==="opencut_project_restore")).toHaveLength(0);
 }finally{f.controller.dispose();}
});

it("reads only the selected fixed versions, labels the real account, and shares its summary with the row heading", async () => {
 const f = await setup(); try {
  await f.controller.scene.bookmark({kind:"put", bookmark:{id:"marker",sequence_id:"scene",time:{ticks:"1",timebase:1},note:"Opening"}});
  const inspect = vi.spyOn(f.api.scene, "inspect");
  vi.spyOn(f.api.scene,"revisionActors").mockResolvedValue({r1:{principal_id:"Editing Assistant",source:"http"}});
  render(<WorkspaceProvider controller={f.controller}><History/></WorkspaceProvider>);
  expect(inspect).not.toHaveBeenCalled();
  fireEvent.click(screen.getAllByRole("button",{name:/^View changes/})[0]);
  await screen.findByRole("list",{name:"Changes"});
  expect(inspect.mock.calls).toEqual([["v4-project","r0"],["v4-project","r1"]]);
  expect(screen.getByText("Account: Editing Assistant · Connected app")).toBeTruthy();
  await waitFor(()=>expect(screen.getAllByText(/Added Marker “Opening”/)).toHaveLength(2));
  const cards=screen.getByRole("list",{name:"Changes"});
  expect(within(cards).getByText("1 s")).toBeTruthy();
  expect(within(cards).getAllByText("Before").length).toBeGreaterThan(0);
  expect(within(cards).getAllByText("After").length).toBeGreaterThan(0);
  expect(screen.queryByRole("table")).toBeNull();
 } finally { f.controller.dispose(); }
});

it("retries a failed snapshot and attribution without displaying internal errors", async () => {
 const f = await setup(); try {
  const inspect = vi.spyOn(f.api.scene,"inspect").mockRejectedValueOnce(Error("/private/source/location"));
  const attribution = vi.spyOn(f.api.scene,"revisionActors").mockRejectedValueOnce(Error("credential error")).mockResolvedValue({r0:null});
  render(<WorkspaceProvider controller={f.controller}><History/></WorkspaceProvider>);
  fireEvent.click(screen.getByRole("button",{name:/^View changes/}));
  await screen.findByText("Could not read these saved changes. Try again.");
  expect(screen.queryByText(/private\/source|credential error/)).toBeNull();
  fireEvent.click(screen.getByRole("button",{name:"Retry"}));
  await screen.findByRole("list",{name:"Changes"});
  expect(inspect).toHaveBeenCalledTimes(2); expect(attribution).toHaveBeenCalledTimes(2);
  expect(screen.getByText("Operator not recorded")).toBeTruthy();
  fireEvent.click(screen.getByRole("button",{name:/^Hide changes/}));
  expect(screen.getByText("Operator not recorded")).toBeTruthy();
 } finally { f.controller.dispose(); }
});

it("discards a late comparison after another row is selected", async () => {
 const f = await setup(); let finish!: (value: Snapshot) => void;
 try {
  await f.controller.scene.bookmark({kind:"put",bookmark:{id:"late",sequence_id:"scene",time:{ticks:"1",timebase:1},note:"Late marker"}});
  const original = f.api.scene.inspect.bind(f.api.scene);
  vi.spyOn(f.api.scene,"inspect").mockImplementation((id, revision) => revision === "r1" ? new Promise(resolve => {finish=resolve;}) : original(id,revision));
  vi.spyOn(f.api.scene,"revisionActors").mockResolvedValue({r0:null,r1:null});
  render(<WorkspaceProvider controller={f.controller}><History/></WorkspaceProvider>);
  fireEvent.click(screen.getAllByRole("button",{name:/^View changes/})[0]);
  await waitFor(()=>expect(finish).toBeTypeOf("function"));
  fireEvent.click(screen.getByRole("button",{name:/^View changes/}));
  await screen.findByRole("list",{name:"Changes"});
  await act(async()=>{finish(f.snapshots.get("r1")!);});
  expect(screen.queryByText(/Late marker/)).toBeNull();
  expect(screen.getByRole<HTMLButtonElement>("button",{name:/^Hide changes/}).getAttribute("aria-expanded")).toBe("true");
 } finally { f.controller.dispose(); }
});

it("discards a late comparison when the connected workspace opens another project", async () => {
 const f = await setup(); let finish!: (value: Snapshot) => void;
 try {
  const old = structuredClone(f.snapshots.get("r0")!);
  const reading=vi.spyOn(f.api.scene,"inspect").mockImplementationOnce(()=>new Promise(resolve=>{finish=resolve;}));
  vi.spyOn(f.api.scene,"revisionActors").mockResolvedValue({r0:{principal_id:"Old account",source:"http"}});
  render(<WorkspaceProvider controller={f.controller}><History/></WorkspaceProvider>);
  fireEvent.click(screen.getByRole("button",{name:/^View changes/}));
  await waitFor(()=>expect(finish).toBeTypeOf("function"));
  reading.mockRestore();
  f.current().project.id="another-project"; f.current().project.name="Another project";
  f.snapshots.set("r0",structuredClone(f.current()));
  vi.mocked(f.api.scene.revisionActors).mockResolvedValue({r0:null});
  await act(async()=>{await f.controller.open(f.ref());});
  await act(async()=>{finish(old);});
  expect(screen.queryByRole("list",{name:"Changes"})).toBeNull();
  expect(screen.queryByText(/Old account|Complete V4/)).toBeNull();
  expect(screen.getByRole("button",{name:/^View changes/})).toBeTruthy();
 } finally { f.controller.dispose(); }
});

it("compares restore against the current version and waits for that comparison before restoring", async () => {
 const f=await setup(); try {
  await f.controller.scene.bookmark({kind:"put",bookmark:{id:"marker",sequence_id:"scene",time:{ticks:"1",timebase:1},note:"Opening"}});
  const inspect=vi.spyOn(f.api.scene,"inspect");
  f.controller.getSnapshot().scene!.history.revisions.find(row=>row.revision==="r0")!.created_at="2026-10-21 16:43:38";
  const localTime=new Date("2026-10-21T16:43:38Z").toLocaleString("en-US");
  vi.spyOn(f.api.scene,"revisionActors").mockResolvedValue({r0:null});
  render(<WorkspaceProvider controller={f.controller}><History/></WorkspaceProvider>);
  expect(screen.getByTitle(localTime).tagName).toBe("TIME");
  fireEvent.click(within(openVersionActions(1)).getByRole("button",{name:"Restore this revision"}));
  const dialog=screen.getByRole("dialog"), confirm=within(dialog).getByRole<HTMLButtonElement>("button",{name:"Restore project"});
  expect(within(dialog).getByText((_,element)=>element?.tagName==="P"&&element.textContent?.includes(localTime)===true)).toBeTruthy();
  expect(confirm.disabled).toBe(true);
  await waitFor(()=>expect(confirm.disabled).toBe(false));
  expect(inspect.mock.calls).toEqual([["v4-project","r1"],["v4-project","r0"]]);
  expect(within(dialog).getAllByText(/Removed Marker “Opening”/)).toHaveLength(2);
  fireEvent.click(confirm);
  await waitFor(()=>expect(f.current().project.sequences[0].bookmarks).toHaveLength(0));
  expect(f.call.mock.calls.filter(([name])=>name==="opencut_project_restore")).toHaveLength(1);
 } finally { f.controller.dispose(); }
});

it("loads attribution for older pages, merges it, and validates attribution identity",async()=>{
 const f=await setup(); try {
  const state=f.controller.getSnapshot().scene!;
  state.history.next_cursor="older"; state.revisionActors={r0:{principal_id:"local-owner",source:"cli"}};
  const original=f.call.getMockImplementation()!;
  f.call.mockImplementation((name,input,options)=>{
   if(name==="opencut_project_history"&&(input as ToolInput<"opencut_project_history">).from_revision==="older") return Promise.resolve({project_id:"v4-project",head_revision:"r0",revisions:[{revision:"older",parent_revision:null,label:"Old edit",created_at:"2026-10-01T00:00:00Z"}],next_cursor:null}) as never;
   if(name==="opencut_project_revision_actors") return Promise.resolve({project_id:"v4-project",actors:{older:{principal_id:"Archive Assistant",source:"http"}}}) as never;
   return original(name,input,options);
  });
  await f.controller.scene.olderHistory();
  expect(f.controller.getSnapshot().scene!.revisionActors).toEqual({r0:{principal_id:"local-owner",source:"cli"},older:{principal_id:"Archive Assistant",source:"http"}});
  expect(f.controller.getSnapshot().scene!.history.revisions.map(row=>row.revision)).toEqual(["r0","older"]);
  expect(f.call.mock.calls).toContainEqual(["opencut_project_revision_actors",{project_id:"v4-project",revisions:["older"]}]);
  f.call.mockResolvedValueOnce({project_id:"wrong-project",actors:{older:null}} as never);
  await expect(f.api.scene.revisionActors("v4-project",["older"])).rejects.toThrow("requested project");
  f.call.mockResolvedValueOnce({project_id:"v4-project",actors:{}} as never);
  await expect(f.api.scene.revisionActors("v4-project",["older"])).rejects.toThrow("requested project");
 }finally{f.controller.dispose();}
});

it("ignores an older page that finishes after disconnect", async()=>{
 const f=await setup(); let finish!: (value: ToolOutput<"opencut_project_history">)=>void;
 try {
  f.controller.getSnapshot().scene!.history.next_cursor="older";
  const original=f.call.getMockImplementation()!;
  f.call.mockImplementation((name,input,options)=>name==="opencut_project_history" ? new Promise(resolve=>{finish=resolve as typeof finish;}) as never : original(name,input,options));
  vi.spyOn(f.api.scene,"revisionActors").mockResolvedValue({});
  const reading=f.controller.scene.olderHistory();
  f.controller.disconnect();
  finish({project_id:"v4-project",head_revision:"r0",revisions:[],next_cursor:null});
  await reading; expect(f.controller.getSnapshot().scene).toBeNull();
 }finally{f.controller.dispose();}
});
