import { lazy, StrictMode, Suspense } from "react";
import { createRoot } from "react-dom/client";
import { isEditorPath } from "./routes";
import "./editor/editor.css";
const Editor = lazy(() =>
  import("./editor/editor-workspace").then((module) => ({
    default: module.EditorWorkspace,
  })),
);
function UnknownPage() {
  return (
    <main style={{ padding: 24 }}>
      <h1>Page not found</h1>
      <a href="/projects">Open projects</a>
    </main>
  );
}
const Page = isEditorPath(location.pathname) ? Editor : UnknownPage;
createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Suspense
      fallback={
        <div role="status" style={{ padding: 24 }}>
          OpenCut…
        </div>
      }
    >
      <Page />
    </Suspense>
  </StrictMode>,
);
