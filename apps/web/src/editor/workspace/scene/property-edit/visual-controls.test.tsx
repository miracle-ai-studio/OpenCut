// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { WorkspaceProvider } from "../../context";
import { setupSourceAudio } from "../source-audio-fixture";
import { preferences } from "../../../shared/locale";
import { VisualPropertyControls } from "./visual-controls";
import { fadeChannel, simpleFades } from "../gain-keyframes/fades";
import type { ToolInput } from "../../../../lib/editor-sdk";
import type { WorkspaceController } from "../../controller";
import { PlayerContext, type PlayerPreview } from "../workbench/player-context";

const controllers: WorkspaceController[] = [];
beforeEach(() => preferences({ language: "en" }));
afterEach(() => { cleanup(); controllers.splice(0).forEach(c => c.dispose()); vi.restoreAllMocks(); });
type Request = ToolInput<"opencut_element_properties_plan">;
async function setup(options: { multi?: boolean; corrupt?: string; delay?: Promise<void>; capabilities?: Promise<readonly string[]>; lost?: boolean; animated?: boolean; locked?: boolean; slider?: boolean; previewDelay?: Promise<void>; restoreDelay?: Promise<boolean> } = {}) {
  const f = await setupSourceAudio(); controllers.push(f.controller);
  const seq = f.current().project.sequences[0], first = seq.tracks[0].elements[0];
  first.visual!.transform.position_y_px = 1.0000000000000002;
  first.visual!.transform.scale_y = 1.0000000000000002;
  const ids = [first.id];
  if (options.multi) {
    const second = structuredClone(first); second.id = "second"; second.visual!.transform.position_x_px = 100;
    second.visual!.opacity = 0.65;
    seq.tracks.push({ ...structuredClone(seq.tracks[0]), id: "second-track", elements: [second] }); ids.push(second.id);
  }
  if (options.animated) {
    const channel = fadeChannel(first, simpleFades(first)!, "fadeIn", "1")!;
    channel.binding = { kind: "property", parameter: "opacity" };
    channel.keys = channel.keys.map((key, i) => ({ ...key, value: i ? 1 : 0 }));
    first.channels = [channel];
  }
  if (options.locked) seq.tracks.at(-1)!.locked = true;
  f.current().revision = "visual-fixture"; f.snapshots.set(f.current().revision, structuredClone(f.current()));
  await f.controller.scene.refresh();
  f.controller.scene.timelineEdit.selectMany(ids.map((id, i) => ({ track_id: i ? "second-track" : "track", element_id: id })));
  vi.mocked(f.api.sdk.hostCapabilities).mockImplementation(async () => ({ tools: options.capabilities ? await options.capabilities : ["opencut_element_properties_plan", ...(options.slider ? ["opencut_visual_preview_edit_draft_frame"] : [])] }) as never);
  const original = f.call.getMockImplementation()!, plans: Request[] = [];
  let accepted: ReturnType<typeof f.commit> | null = null;
  f.call.mockImplementation(async (name, input, opts) => {
    if (name === "opencut_edit_apply" && options.lost) {
      if (accepted) return accepted as never;
      accepted = f.commit(input as ToolInput<"opencut_edit_apply">); throw Error("Connection interrupted after acceptance");
    }
    if (name !== "opencut_element_properties_plan") return original(name, input, opts);
    const request = input as Request; plans.push(structuredClone(request));
    const track = f.current().project.sequences[0].tracks.find(t => t.elements.some(e => e.id === request.element_id))!;
    const element = structuredClone(track.elements.find(e => e.id === request.element_id)!);
    if (request.transform) element.visual!.transform = request.transform;
    if (request.opacity !== null) element.visual!.opacity = request.opacity!;
    const plan = { project_id: request.project_id, expected_revision: request.expected_revision,
      idempotency_key: request.idempotency_key, label: request.label, operations: [{
        type: "element_put" as const, sequence_id: request.sequence_id, track_id: track.id, element }] };
    if (options.corrupt && plans.length === (options.multi ? 2 : 1)) {
      if (options.corrupt === "element") element.enabled = !element.enabled;
      if (options.corrupt === "sequence") plan.operations[0].sequence_id = "other";
      if (options.corrupt === "revision") plan.expected_revision = "foreign";
      if (options.corrupt === "key") plan.idempotency_key = "foreign";
    }
    await options.delay; return plan as never;
  });
  const applyOriginal = f.controller.scene.applyPlanned.bind(f.controller.scene);
  const apply = vi.spyOn(f.controller.scene, "applyPlanned");
  const preview = vi.spyOn(f.controller.scene.visualPreview, "previewEditDraft");
  const restorePicture = vi.spyOn(f.controller.scene.visualPreview, "clearEditDraft");
  if (options.slider) {
    const state = f.controller.getSnapshot().scene!;
    state.audioPreview = {...state.audioPreview, phase:"paused", session:{
      session_id:"preview", generation:"1", overlay_epoch:"0", owner_id:"owner", lease_id:"lease",
    } as NonNullable<typeof state.audioPreview.session>};
    const picture = {...f.controller.scene.visualPreview.snapshot(), phase:"ready" as const};
    vi.spyOn(f.controller.scene.visualPreview, "snapshot").mockReturnValue(picture);
    vi.spyOn(f.controller.scene.visualPreview, "pausedEditSession").mockImplementation(() => f.controller.getSnapshot().scene!.audioPreview.session);
    preview.mockImplementation(async () => { await options.previewDelay; });
    restorePicture.mockImplementation(async () => options.restoreDelay ? await options.restoreDelay : true);
  }
  const player = {preview:null as PlayerPreview|null, open:async()=>{}, close:()=>{},
    begin:async()=>true, release:()=>{}, stopExternal:vi.fn()};
  const tree = () => <WorkspaceProvider controller={f.controller}><PlayerContext.Provider value={{...player}}><VisualPropertyControls elementIds={ids} /></PlayerContext.Provider></WorkspaceProvider>;
  const view = render(tree());
  const playerPreview = (preview:PlayerPreview|null) => {player.preview=preview;view.rerender(tree());};
  if (!options.capabilities && !options.locked) await waitFor(() => expect(input("Horizontal position (pixels)").disabled).toBe(false));
  if (options.locked) await waitFor(() => expect(screen.queryByRole("status")).toBeNull());
  return { ...f, seq, first, ids, plans, apply, applyOriginal, view, preview, restorePicture, playerPreview };
}
const input = (name: string) => screen.getByLabelText(name) as HTMLInputElement;
const change = (name: string, value: string) => fireEvent.change(input(name), { target: { value } });
const enter = (name: string) => fireEvent.keyDown(input(name), { key: "Enter" });
const x = "Horizontal position (pixels)", y = "Vertical position (pixels)", opacity = "Opacity (percent)";
const slider = () => screen.getByRole("slider", {name:"Preview opacity"}) as HTMLInputElement;
const drag = (value: string) => {
  fireEvent.pointerDown(slider(), {pointerId:1, button:0});
  fireEvent.change(slider(), {target:{value}});
};

it("previews opacity without applying; Escape waits for actual restoration and preserves another field draft", async () => {
  let restored!: (value:boolean) => void;
  const restoreDelay = new Promise<boolean>(resolve => {restored=resolve;});
  const f = await setup({slider:true, restoreDelay});
  change(y,"72"); drag("75");
  await waitFor(() => expect(f.preview).toHaveBeenCalledOnce());
  expect(f.preview.mock.calls[0][0]).toMatchObject({intent:{kind:"property",property:"opacity"}, transaction:{operations:[{element:{visual:{opacity:0.75}}}]}});
  expect(f.apply).not.toHaveBeenCalled();
  fireEvent.keyDown(slider(), {key:"Escape"});
  expect(screen.getByText("Restoring the original picture…")).toBeTruthy();
  expect(f.controller.getSnapshot().scene!.propertyDrafts).toHaveLength(1);
  await act(async () => restored(true));
  await waitFor(() => expect(input(opacity).value).toBe("100"));
  expect(input(y).value).toBe("72"); expect(f.apply).not.toHaveBeenCalled();
  expect(f.controller.getSnapshot().scene!.propertyDrafts).toHaveLength(1);
});
it("release waits for the exact final Native preview then applies its original request once", async () => {
  let done!: () => void; const previewDelay=new Promise<void>(resolve => {done=resolve;});
  const f = await setup({slider:true, previewDelay}); drag("75");
  await waitFor(() => expect(f.preview).toHaveBeenCalledOnce());
  fireEvent.pointerUp(slider(),{pointerId:1}); expect(f.apply).not.toHaveBeenCalled();
  await act(async () => done());
  await waitFor(() => expect(f.apply).toHaveBeenCalledOnce());
  expect(f.apply.mock.calls[0][0]).toEqual(f.preview.mock.calls[0][0].transaction);
  expect(f.current().project.sequences[0].tracks[0].elements[0].visual!.opacity).toBe(.75);
  expect(f.plans).toHaveLength(1);
});
it.each(["blur","pointercancel","lostcapture"])("%s cancels a held opacity gesture without applying", async exit => {
  const f=await setup({slider:true}); drag("75"); await waitFor(() => expect(f.preview).toHaveBeenCalledOnce());
  if(exit==="blur") fireEvent(window,new Event("blur"));
  else if(exit==="pointercancel") fireEvent.pointerCancel(slider(),{pointerId:1});
  else fireEvent.lostPointerCapture(slider(),{pointerId:1});
  await waitFor(() => expect(input(opacity).value).toBe("100"));
  expect(f.apply).not.toHaveBeenCalled();
});
it("coalesces obsolete opacity work and commits only the released final value", async () => {
  let done!: () => void; const previewDelay=new Promise<void>(resolve => {done=resolve;});
  const f=await setup({slider:true, previewDelay}); drag("60");
  await waitFor(() => expect(f.preview).toHaveBeenCalledOnce());
  fireEvent.change(slider(),{target:{value:"40"}}); fireEvent.change(slider(),{target:{value:"75"}});
  fireEvent.pointerUp(slider(),{pointerId:1}); expect(f.apply).not.toHaveBeenCalled();
  await act(async () => done()); await waitFor(() => expect(f.apply).toHaveBeenCalledOnce());
  expect(f.preview).toHaveBeenCalledTimes(2);
  expect(f.apply.mock.calls[0][0].operations).toMatchObject([{element:{visual:{opacity:.75}}}]);
  expect(f.plans.map(p=>p.opacity)).toEqual([.6,.75]);
});
it("owns an unchanged gesture immediately and groups repeated keyboard changes into one commit", async () => {
  const f=await setup({slider:true});
  fireEvent.pointerDown(slider(),{pointerId:1,button:0});
  expect(f.controller.getSnapshot().scene!.propertyDrafts).toHaveLength(1);
  fireEvent.pointerCancel(slider(),{pointerId:1});
  await waitFor(()=>expect(f.controller.getSnapshot().scene!.propertyDrafts).toEqual([]));
  fireEvent.keyDown(slider(),{key:"ArrowLeft"}); fireEvent.change(slider(),{target:{value:"99"}});
  fireEvent.keyDown(slider(),{key:"ArrowLeft",repeat:true}); fireEvent.change(slider(),{target:{value:"98"}});
  fireEvent.keyUp(slider(),{key:"ArrowLeft"});
  await waitFor(()=>expect(f.apply).toHaveBeenCalledOnce());
  expect(f.apply.mock.calls[0][0].operations).toMatchObject([{element:{visual:{opacity:.98}}}]);
});

it("cancels when the visible player changes context and disables hidden timeline opacity preview", async () => {
  const f=await setup({slider:true}); drag("75"); await waitFor(()=>expect(f.preview).toHaveBeenCalledOnce());
  f.playerPreview({kind:"effect",name:"Blur",intensity:1});
  await waitFor(()=>expect(input(opacity).value).toBe("100"));
  expect(f.apply).not.toHaveBeenCalled(); expect(slider().disabled).toBe(true);
  expect(screen.getByText("Return to the timeline picture to preview opacity.")).toBeTruthy();
});

it("renders real mixed values, not zero, and does not commit rounded untouched precision", async () => {
  const f = await setup({ multi: true });
  expect(input(x).value).toBe(""); expect(input(x).placeholder).toBe("—"); expect(input(opacity).value).toBe("");
  expect(input("Vertical scale (percent)").value).toBe("100"); enter("Vertical scale (percent)");
  expect(f.plans).toHaveLength(0); expect(f.apply).not.toHaveBeenCalled();
  expect(f.first.visual!.transform.scale_y).toBe(1.0000000000000002);
});

it("confirms only the current field, preserves the other draft across its known own refresh and restores its owner and focus", async () => {
  const f = await setup({ multi: true }), before = structuredClone(f.current().project);
  change(x, "25.5"); change(y, "72"); input(x).focus(); enter(x);
  await waitFor(() => expect(f.apply).toHaveBeenCalledOnce());
  await waitFor(() => expect(input(x).disabled).toBe(false));
  expect(document.activeElement).toBe(input(x));
  expect(input(y).value).toBe("72"); expect(f.controller.getSnapshot().scene!.propertyDrafts).toHaveLength(1);
  for (const [i, track] of f.current().project.sequences[0].tracks.entries()) {
    const next = structuredClone(track.elements[0]); next.visual!.transform.position_x_px = before.sequences[0].tracks[i].elements[0].visual!.transform.position_x_px;
    expect(next).toEqual(before.sequences[0].tracks[i].elements[0]);
  }
  const afterX = f.current().revision;
  enter(y); await waitFor(() => expect(f.apply).toHaveBeenCalledTimes(2));
  await waitFor(() => expect(f.controller.getSnapshot().scene!.propertyDrafts).toEqual([]));
  expect(f.plans.slice(2).every(p => p.expected_revision === afterX)).toBe(true);
  expect(f.current().project.sequences[0].tracks.every(t => t.elements[0].visual!.transform.position_y_px === 72)).toBe(true);
});

it("Esc restores only this field and does not clear the cohort or another field's draft", async () => {
  const f = await setup({ multi: true }); change(x, "25"); change(y, "72");
  fireEvent.keyDown(input(x), { key: "Escape" });
  expect(input(x).value).toBe(""); expect(input(y).value).toBe("72");
  expect(f.controller.getSnapshot().scene!.timelineEdit.selection).toHaveLength(2);
  expect(f.controller.getSnapshot().scene!.propertyDrafts).toHaveLength(1);
  fireEvent.keyDown(input(y), { key: "Escape" }); expect(f.controller.getSnapshot().scene!.propertyDrafts).toEqual([]);
  expect(f.plans).toHaveLength(0); expect(f.apply).not.toHaveBeenCalled();
});

it.each([["Horizontal scale (percent)", "-150", -1.5], ["Rotation (degrees)", "725", 725], [opacity, "75", 0.75]] as const)("writes %s with native units and preserves other authored properties", async (field, text, expected) => {
  const f = await setup(); change(field, text); enter(field);
  await waitFor(() => expect(f.apply).toHaveBeenCalledOnce());
  await waitFor(() => expect(f.controller.getSnapshot().scene!.propertyDrafts).toEqual([]));
  const visual = f.current().project.sequences[0].tracks[0].elements[0].visual!;
  expect(field === opacity ? visual.opacity : field === "Rotation (degrees)" ? visual.transform.rotation_degrees : visual.transform.scale_x).toBe(expected);
  expect(f.plans[0].audio_gain).toBeNull(); expect(f.plans[0].retime).toBeNull();
  expect(f.current().project.sequences[0].tracks[0].elements[0].visual!.transform.position_y_px).toBe(1.0000000000000002);
});

it("reports opacity bounds directly without planning, and composition Enter cannot submit", async () => {
  const f = await setup(); change(opacity, "101"); enter(opacity);
  expect(screen.getByRole("alert").textContent).toBe("Opacity must be between 0% and 100%.");
  expect(input(opacity).getAttribute("aria-invalid")).toBe("true");
  expect(input(opacity).getAttribute("aria-describedby")).toBe(screen.getByRole("alert").id);
  change(opacity, "75"); fireEvent.compositionStart(input(opacity)); enter(opacity); fireEvent.compositionEnd(input(opacity));
  expect(f.plans).toHaveLength(0); expect(f.apply).not.toHaveBeenCalled();
});

it.each(["element", "sequence", "revision", "key"])("rejects a second target's corrupt %s plan before any atomic apply", async corrupt => {
  const f = await setup({ multi: true, corrupt }); change(x, "25"); enter(x);
  await screen.findByText("The picture property plan changed fields outside the requested edit.");
  expect(f.plans).toHaveLength(2); expect(f.apply).not.toHaveBeenCalled();
  expect(f.current().revision).toBe("visual-fixture");
});

it("keeps dirty text frozen across an external head while a plan is pending, then allows Esc without rebasing or applying", async () => {
  let release!: () => void; const delay = new Promise<void>(done => { release = done; });
  const f = await setup({ delay }); change(x, "25"); change(y, "72"); enter(x);
  await waitFor(() => expect(f.plans).toHaveLength(1));
  f.current().revision = "external"; f.snapshots.set("external", structuredClone(f.current()));
  await act(async () => { await f.controller.scene.refresh(); release(); });
  await waitFor(() => expect(input(x).readOnly).toBe(true));
  expect(input(x).value).toBe("25"); expect(input(y).value).toBe("72"); expect(f.apply).not.toHaveBeenCalled();
  fireEvent.keyDown(input(x), { key: "Escape" }); expect(input(y).value).toBe("72");
});

it("hands accepted-but-lost writes to exact shared recovery without starting another request or reclaiming its owner", async () => {
  const f = await setup({ lost: true }); change(x, "25"); change(y, "72"); enter(x);
  await waitFor(() => expect(f.controller.getSnapshot().scene!.recovery?.record).toBeTruthy());
  await waitFor(() => expect(f.controller.getSnapshot().scene!.propertyDrafts).toEqual([]));
  const record = f.controller.getSnapshot().scene!.recovery!.record!, planned = f.apply.mock.calls[0][0];
  expect(record.context.intent.kind).toBe("public_apply"); expect(record.context.intent.request).toEqual(planned);
  enter(y); expect(f.plans).toHaveLength(1); expect(f.apply).toHaveBeenCalledOnce();
  expect(input(y).value).toBe("72");
  await act(async () => { await f.controller.scene.recovery.retry(); });
  await waitFor(() => expect(f.controller.getSnapshot().scene!.recovery?.record).toBeNull());
  await waitFor(() => expect(f.controller.getSnapshot().scene!.propertyDrafts).toHaveLength(1));
  const selection = structuredClone(f.controller.getSnapshot().scene!.timelineEdit.selection);
  act(() => f.controller.scene.timelineEdit.select({ track_id: "track", element_id: "missing" }));
  expect(f.controller.getSnapshot().scene!.timelineEdit.selection).toEqual(selection);
  expect(input(y).value).toBe("72"); expect(input(y).readOnly).toBe(true);
  fireEvent.keyDown(input(x), { key: "Escape" });
  expect(f.controller.getSnapshot().scene!.propertyDrafts).toHaveLength(1);
  fireEvent.keyDown(input(y), { key: "Escape" });
  expect(f.controller.getSnapshot().scene!.propertyDrafts).toEqual([]);
  await act(async () => { await f.controller.scene.refresh(); });
  await waitFor(() => expect(input(x).disabled).toBe(false));
  expect(input(x).value).toBe("25"); expect(input(y).value).toBe("1");
  expect(f.plans).toHaveLength(1);
  const writes = f.call.mock.calls.filter(([name]) => name === "opencut_edit_apply");
  expect(writes).toHaveLength(2); expect(writes[0][1]).toEqual(writes[1][1]);
});

it("does not rebase other field drafts when a foreign successor follows its own accepted write", async () => {
  const f = await setup(), original = f.applyOriginal;
  f.apply.mockImplementation(async (...args) => {
    const receipt = await original(...args);
    f.current().revision = "foreign-successor";
    f.current().project.sequences[0].tracks[0].elements[0].visual!.transform.position_y_px = 99;
    f.snapshots.set(f.current().revision, structuredClone(f.current()));
    await f.controller.scene.refresh(); return receipt;
  });
  change(x, "25"); change(y, "72"); enter(x);
  await waitFor(() => {
    expect(f.current().revision).toBe("foreign-successor");
    expect(input(y).disabled).toBe(false); expect(input(y).readOnly).toBe(true);
  });
  expect(input(y).value).toBe("72"); expect(f.controller.getSnapshot().scene!.propertyDrafts).toHaveLength(1);
  enter(y); expect(f.plans).toHaveLength(1); expect(f.apply).toHaveBeenCalledOnce();
  expect(f.current().project.sequences[0].tracks[0].elements[0].visual!.transform.position_y_px).toBe(99);
});

it("rejects a delayed property plan after disconnect without applying or re-registering an orphan draft", async () => {
  let release!: () => void; const delay = new Promise<void>(done => { release = done; });
  const f = await setup({ delay }); change(x, "25"); enter(x);
  await waitFor(() => expect(f.plans).toHaveLength(1));
  await act(async () => { f.controller.disconnect(); release(); });
  expect(f.apply).not.toHaveBeenCalled(); expect(f.controller.getSnapshot().scene).toBeNull();
});

it("does not present capability initialization as a changed target or enable fields prematurely", async () => {
  let resolve!: (tools: readonly string[]) => void;
  const capabilities = new Promise<readonly string[]>(done => { resolve = done; });
  const f = await setup({ capabilities });
  expect(screen.getByRole("status").textContent).toBe("Checking picture editing availability…");
  expect(input(x).disabled).toBe(true);
  await act(async () => { resolve(["opencut_element_properties_plan"]); });
  await waitFor(() => expect(input(x).disabled).toBe(false));
  expect(screen.queryByText("The property target changed. Confirm no more fields; restore your drafts before refreshing.")).toBeNull();
  expect(f.plans).toHaveLength(0);
});

it("keeps animated fields in keyframe editing while preserving their exact channels when another common field is saved", async () => {
  const f = await setup({ multi: true, animated: true });
  const channels = structuredClone(f.first.channels);
  expect(input(opacity).disabled).toBe(true); expect(input(x).disabled).toBe(false);
  expect(document.getElementById(input(opacity).getAttribute("aria-describedby")!)?.textContent).toBe("Edit this animated property with the keyframe controls.");
  change(x, "25"); enter(x);
  await waitFor(() => expect(f.controller.getSnapshot().scene!.propertyDrafts).toEqual([]));
  expect(f.current().project.sequences[0].tracks[0].elements[0].channels).toEqual(channels);
  expect(f.apply).toHaveBeenCalledOnce();
});

it("does not silently omit a locked member from a multi-target edit", async () => {
  const f = await setup({ multi: true, locked: true });
  expect(input(x).disabled).toBe(true);
  expect(screen.getByText("The property target changed or its track is locked.")).toBeTruthy();
  expect(f.plans).toHaveLength(0); expect(f.apply).not.toHaveBeenCalled();
});

it("keeps pristine properties usable after a failed history plan changes the view generation without changing the head", async () => {
  const f = await setup(), before = f.controller.getSnapshot().scene!.head.revision;
  const original = f.call.getMockImplementation()!;
  f.call.mockImplementation(async (name, args, opts) => {
    if (name === "opencut_edit_undo_plan") throw Error("History planning unavailable");
    return original(name, args, opts);
  });
  await act(async () => { await expect(f.controller.scene.planUndo("unavailable")).rejects.toThrow("History planning unavailable"); });
  expect(f.controller.getSnapshot().scene!.head.revision).toBe(before);
  await waitFor(() => expect(input(x).disabled).toBe(false));
  change(x, "25"); enter(x);
  await waitFor(() => expect(f.apply).toHaveBeenCalledOnce());
  await waitFor(() => expect(f.controller.getSnapshot().scene!.propertyDrafts).toEqual([]));
});
