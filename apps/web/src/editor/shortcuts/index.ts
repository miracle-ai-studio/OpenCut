export {
  EditorShortcutsProvider,
  useShortcutActions,
  useShortcuts,
  useShortcutLabels,
} from "./provider";
export { ShortcutsButton, ShortcutSettingsDialog } from "./dialog";
export { V4WorkspaceShortcutActions } from "./v4-actions";
export { ACTIONS, actionDefinition } from "./catalog";
export type { ShortcutActionId, ShortcutBindings } from "./catalog";
export type {
  ShortcutHandlers,
  ShortcutHandler,
  ShortcutAvailability,
} from "./registry";
