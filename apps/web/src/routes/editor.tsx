import { createFileRoute } from "@tanstack/react-router";
import { EditorWorkspace } from "../editor/editor-workspace";

export const Route = createFileRoute("/editor")({ component: Editor });

function Editor() {
  return <EditorWorkspace />;
}
