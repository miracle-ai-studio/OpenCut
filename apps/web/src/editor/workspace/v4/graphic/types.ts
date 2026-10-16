import type { ToolInput, ToolOutput } from "../../../../lib/editor-sdk";
export type GraphicPlanRequest = ToolInput<"opencut_graphic_v1_plan">;
export type GraphicPlanResult = ToolOutput<"opencut_graphic_v1_plan">;
export type GraphicInitial = NonNullable<GraphicPlanResult["initial"]>;
export type GraphicValues = Extract<
  GraphicPlanRequest["command"],
  { kind: "create" }
>["values"];
export type GraphicKind = Extract<
  GraphicPlanRequest["command"],
  { kind: "defaults" }
>["geometry"];
export const graphicKinds = ["rectangle", "ellipse", "polygon", "star"] as const satisfies readonly GraphicKind[];
export type GraphicScope = {
  projectId: string;
  revision: string;
  sequenceId: string;
};
