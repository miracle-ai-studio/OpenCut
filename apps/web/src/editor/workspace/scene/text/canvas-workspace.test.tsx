// @vitest-environment jsdom
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { WorkspaceProvider } from "../../context";
import { setup as createFixture } from "../fixture";
import { textInitial, textFonts } from "./fixture";
import { TextAuthoringProvider, useCanvasTextAuthoring } from "./workspace";
import type { CanvasTextPlanning, CanvasTextPlan } from "./canvas-types";
function Surface() {
  const c = useCanvasTextAuthoring()!;
  return (
    <>
      <button onClick={() => c.beginCanvasEdit("image")}>Begin</button>
      {c.canvas && (
        <>
          <span>Canvas draft</span>
          <button onClick={c.cancelCanvas}>Discard</button>
        </>
      )}
    </>
  );
}
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});
async function setup(customPlanner?: CanvasTextPlanning) {
  const f = await createFixture();
  const track = f.current().project.sequences[0].tracks[0];
  track.kind = "text";
  track.elements[0].content = { kind: "text", text: textInitial().text };
  f.current().project.fonts = textFonts();
  await f.controller.scene.refresh();
  const planner: CanvasTextPlanning = customPlanner ?? vi.fn(
    async (scope) =>
      ({
        scope,
        width: 320,
        height: 180,
        block: { maxWidth: 80, height: 72 },
        layer: { elementId: "image" },
      }) as CanvasTextPlan,
  );
  render(
    <WorkspaceProvider controller={f.controller}>
      <TextAuthoringProvider available planCanvasText={planner}>
        <Surface />
      </TextAuthoringProvider>
    </WorkspaceProvider>,
  );
  return f;
}
it("canvas drafting and Escape cancellation leave fixed head without a mutation plan", async () => {
  const f = await setup();
  try {
    const apply = vi.spyOn(f.controller.scene, "applyPlanned");
    fireEvent.click(screen.getByText("Begin"));
    await screen.findByText("Canvas draft");
    expect(f.current().revision).toBe("r0");
    fireEvent.click(screen.getByText("Discard"));
    expect(screen.queryByText("Canvas draft")).toBeNull();
    expect(apply).not.toHaveBeenCalled();
    expect(
      f.call.mock.calls.filter(([name]) => name === "opencut_text_plan"),
    ).toHaveLength(0);
  } finally {
    f.controller.dispose();
  }
});
it("begins the actual-alpha double-click edit after its selection transition without retiring the new plan", async () => {
  const f = await setup();
  try {
    const apply = vi.spyOn(f.controller.scene, "applyPlanned");
    await act(async () => {
      f.controller.scene.select("image");
      fireEvent.click(screen.getByText("Begin"));
    });
    await screen.findByText("Canvas draft");
    expect(f.controller.getSnapshot().scene?.selectedId).toBe("image");
    expect(f.current().revision).toBe("r0");
    expect(apply).not.toHaveBeenCalled();
  } finally {
    f.controller.dispose();
  }
});
it("external fixed head change retires the canvas draft before commit", async () => {
  const f = await setup();
  try {
    fireEvent.click(screen.getByText("Begin"));
    await screen.findByText("Canvas draft");
    await act(async () => {
      f.current().revision = "external";
      await f.controller.scene.refresh();
    });
    await waitFor(() => expect(screen.queryByText("Canvas draft")).toBeNull());
  } finally {
    f.controller.dispose();
  }
});

it("seek retires both an open canvas draft and a late native text plan without a write", async () => {
  let resolve!: (plan: CanvasTextPlan)=>void;
  let captured!: CanvasTextPlan["scope"];
  const planner=vi.fn<CanvasTextPlanning>((scope)=>{captured=scope;return new Promise(done=>{resolve=done;});});
  const f=await setup(planner);
  try {
    const apply=vi.spyOn(f.controller.scene,"applyPlanned");
    fireEvent.click(screen.getByText("Begin"));
    await waitFor(()=>expect(planner).toHaveBeenCalledTimes(1));
    await act(async()=>{f.controller.scene.seek({ticks:"1",timebase:1});resolve({scope:captured,width:320,height:180,block:{maxWidth:80,height:72},layer:{elementId:"image"}} as CanvasTextPlan);});
    expect(screen.queryByText("Canvas draft")).toBeNull();expect(apply).not.toHaveBeenCalled();expect(f.current().revision).toBe("r0");
  }finally{f.controller.dispose();}
});
