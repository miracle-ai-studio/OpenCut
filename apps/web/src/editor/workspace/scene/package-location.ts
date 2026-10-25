import type { ToolInput } from "../../../lib/editor-sdk";
import type { EditorApi } from "../api";

type Selection = ToolInput<"opencut_project_package_choose_location">;

export function packageFileName(projectName: string) {
  const cleaned = projectName.replace(/[\\/:*?"<>|\u0000-\u001f\u007f]/g, "-").trim().replace(/^[. ]+|[. ]+$/g, "");
  let name = "";
  for (const point of cleaned || "OpenCut") {
    if (new TextEncoder().encode(name + point).length > 210) break;
    name += point;
  }
  return `${name}.opencut`;
}

export async function choosePackageLocation(api: EditorApi, input: Selection) {
  const result = await api.sdk.call("opencut_project_package_choose_location", input);
  if (!result || Object.keys(result).join(",") !== "path" ||
      (result.path !== null && (typeof result.path !== "string" || !result.path.startsWith("/") || /[\u0000-\u001f\u007f]/.test(result.path)))) {
    throw Error("The selected package location could not be read. Choose it again.");
  }
  return result.path;
}
