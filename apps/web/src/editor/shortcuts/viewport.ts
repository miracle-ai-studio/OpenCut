import { usePreviewViewport as useViewport } from "../shared/preview-viewport";
import { useShortcutActions } from "./provider";
export { ViewportControls } from "../shared/preview-viewport";

/** Register view tools beside the shared dispatcher, outside foundation/state. */
export function usePreviewViewport(...args: Parameters<typeof useViewport>) {
  const viewport = useViewport(...args);
  useShortcutActions({
    "pan-preview": { run: viewport.holdPan, release: viewport.releasePan },
  });
  return viewport;
}
