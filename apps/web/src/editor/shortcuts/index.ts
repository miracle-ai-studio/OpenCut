export {
  EditorShortcutsProvider,
  useShortcutActions,
  useShortcuts,
  useShortcutLabels,
} from "./provider";
export { ShortcutsButton, ShortcutSettingsDialog } from "./dialog";
export { WorkspaceShortcutActions } from "./scene-actions";
export { ACTIONS, actionDefinition } from "./catalog";
export type { ShortcutActionId, ShortcutBindings } from "./catalog";
export type {
  ShortcutHandlers,
  ShortcutHandler,
  ShortcutAvailability,
} from "./registry";
