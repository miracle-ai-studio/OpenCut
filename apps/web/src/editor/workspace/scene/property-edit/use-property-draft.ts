import { useId, useLayoutEffect } from "react";
import type { WorkspaceController } from "../../controller";

/** Keep a dirty inspector field visible to every editor write, without putting
 * a second document or mutation queue in a component. */
export function usePropertyDraft(
  controller: WorkspaceController,
  dirty: boolean,
) {
  const owner = useId();
  useLayoutEffect(() => {
    controller.scene.propertyDraft(owner, dirty);
  }, [controller, owner, dirty]);
  useLayoutEffect(
    () => () => controller.scene.propertyDraft(owner, false),
    [controller, owner],
  );
  return owner;
}
