import { lazy, StrictMode, Suspense } from "react";
import { createRoot } from "react-dom/client";
import { isEditorPath } from "./routes";
import "./editor/editor.css";
const Editor = lazy(() =>
  import("./editor/editor-workspace").then((module) => ({
    default: module.EditorWorkspace,
  })),
);
const Site = lazy(() =>
  import("./site/site").then((module) => ({ default: module.Site })),
);
const Page = isEditorPath(location.pathname) ? Editor : Site;
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
