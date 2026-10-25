/** Action names and default physical keys follow Classic's actions/definitions.ts.
 * Handlers are registered by the owning UI and call its existing controller.
 * `mod` means Command on Apple platforms and Control elsewhere. */
export const SHORTCUT_ACTIONS = [
  {
    id: "toggle-play",
    label: "Play / pause",
    category: "Playback",
    keys: ["space", "k"],
  },
  {
    id: "stop-playback",
    label: "Stop playback",
    category: "Playback",
    keys: [],
  },
  {
    id: "seek-forward",
    label: "Seek forward 1 second",
    category: "Playback",
    keys: ["l"],
    repeat: true,
  },
  {
    id: "seek-backward",
    label: "Seek backward 1 second",
    category: "Playback",
    keys: ["j"],
    repeat: true,
  },
  {
    id: "frame-step-forward",
    label: "Next frame",
    category: "Navigation",
    keys: ["right"],
    repeat: true,
  },
  {
    id: "frame-step-backward",
    label: "Previous frame",
    category: "Navigation",
    keys: ["left"],
    repeat: true,
  },
  {
    id: "jump-forward",
    label: "Jump forward 5 seconds",
    category: "Navigation",
    keys: ["shift+right"],
    repeat: true,
  },
  {
    id: "jump-backward",
    label: "Jump backward 5 seconds",
    category: "Navigation",
    keys: ["shift+left"],
    repeat: true,
  },
  {
    id: "goto-start",
    label: "Go to timeline start",
    category: "Navigation",
    keys: ["home", "enter"],
  },
  {
    id: "goto-end",
    label: "Go to timeline end",
    category: "Navigation",
    keys: ["end"],
  },
  {
    id: "split",
    label: "Split elements at playhead",
    category: "Editing",
    keys: ["s"],
    scope: "timeline",
  },
  {
    id: "split-left",
    label: "Split and remove left",
    category: "Editing",
    keys: ["q"],
    scope: "timeline",
  },
  {
    id: "split-right",
    label: "Split and remove right",
    category: "Editing",
    keys: ["w"],
    scope: "timeline",
  },
  {
    id: "delete-selected",
    label: "Delete current selection",
    category: "Editing",
    keys: ["backspace", "delete"],
    scope: "timeline",
  },
  {
    id: "copy-selected",
    label: "Copy selected elements",
    category: "Editing",
    keys: ["mod+c"],
    scope: "timeline",
  },
  {
    id: "paste-copied",
    label: "Paste elements at playhead",
    category: "Editing",
    keys: ["mod+v"],
    scope: "timeline",
  },
  {
    id: "duplicate-selected",
    label: "Duplicate selected elements",
    category: "Editing",
    keys: ["mod+d"],
    scope: "timeline",
  },
  {
    id: "toggle-snapping",
    label: "Toggle snapping",
    category: "Timeline",
    keys: ["n"],
    scope: "timeline",
  },
  {
    id: "toggle-ripple-editing",
    label: "Toggle ripple editing",
    category: "Timeline",
    keys: [],
    scope: "timeline",
  },
  {
    id: "toggle-source-audio",
    label: "Extract or recover source audio",
    category: "Editing",
    keys: [],
    scope: "timeline",
  },
  {
    id: "select-all",
    label: "Select all elements",
    category: "Selection",
    keys: ["mod+a"],
    scope: "timeline",
  },
  {
    id: "deselect-all",
    label: "Deselect all elements",
    category: "Selection",
    keys: [],
    scope: "timeline",
  },
  {
    id: "cancel-interaction",
    label: "Cancel current interaction",
    category: "Selection",
    keys: ["escape"],
  },
  {
    id: "toggle-elements-muted-selected",
    label: "Mute / unmute selected elements",
    category: "Editing",
    keys: [],
    scope: "timeline",
  },
  {
    id: "toggle-elements-visibility-selected",
    label: "Show / hide selected elements",
    category: "Editing",
    keys: [],
    scope: "timeline",
  },
  {
    id: "toggle-bookmark",
    label: "Toggle bookmark at playhead",
    category: "Timeline",
    keys: ["m"],
    scope: "timeline",
  },
  { id: "undo", label: "Undo", category: "History", keys: ["mod+z"] },
  {
    id: "redo",
    label: "Redo",
    category: "History",
    keys: ["mod+shift+z", "mod+y"],
  },
  {
    id: "remove-media-asset",
    label: "Remove selected media asset",
    category: "Assets",
    keys: [],
    scope: "assets",
  },
  {
    id: "remove-media-assets",
    label: "Remove selected media assets",
    category: "Assets",
    keys: [],
    scope: "assets",
  },
  {
    id: "zoom-in",
    label: "Zoom timeline in",
    category: "Timeline",
    keys: [],
    scope: "timeline",
    repeat: true,
  },
  {
    id: "zoom-out",
    label: "Zoom timeline out",
    category: "Timeline",
    keys: [],
    scope: "timeline",
    repeat: true,
  },
  {
    id: "zoom-selection",
    label: "Zoom to selection",
    category: "Timeline",
    keys: [],
    scope: "timeline",
  },
  {
    id: "pan-preview",
    label: "Pan preview (hold)",
    category: "Navigation",
    keys: ["h"],
    scope: "canvas",
    hold: true,
  },
  {
    id: "show-shortcuts",
    label: "Keyboard shortcuts",
    category: "Help",
    keys: ["shift+/"],
  },
] as const;

export type ShortcutActionId = (typeof SHORTCUT_ACTIONS)[number]["id"];
export type ShortcutScope = "workspace" | "timeline" | "assets" | "canvas";
export type ShortcutDefinition = {
  id: ShortcutActionId;
  label: string;
  category: string;
  keys: readonly string[];
  scope?: ShortcutScope;
  repeat?: boolean;
  hold?: boolean;
};
export type ShortcutBindings = Record<ShortcutActionId, string[]>;
export const ACTIONS: readonly ShortcutDefinition[] = SHORTCUT_ACTIONS;
export const actionDefinition = (id: ShortcutActionId) =>
  ACTIONS.find((action) => action.id === id)!;
export const isShortcutAction = (id: string): id is ShortcutActionId =>
  ACTIONS.some((action) => action.id === id);
export const defaultBindings = (): ShortcutBindings =>
  Object.fromEntries(
    ACTIONS.map((action) => [action.id, [...action.keys]]),
  ) as ShortcutBindings;
