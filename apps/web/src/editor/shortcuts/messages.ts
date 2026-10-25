import { friendlyCopy } from "../shared/friendly-copy";
import { useLocale, t } from "../shared/locale";

const zh: Record<string, string> = {
  "Pan preview (hold)": "按住以平移预览",
  "Preview canvas": "预览画布",
  "Middle drag to pan. Hold {key} and drag to use the hand tool.":
    "鼠标中键拖动可平移；按住 {key} 拖动也可使用抓手。",
  "Keyboard shortcuts": "键盘快捷键",
  "Shortcuts and settings": "快捷键与设置",
  Playback: "播放",
  Navigation: "导航",
  Editing: "编辑",
  Timeline: "时间线",
  Selection: "选择",
  History: "历史",
  Assets: "素材",
  Help: "帮助",
  "Play / pause": "播放 / 暂停",
  "Stop playback": "停止播放",
  "Seek forward 1 second": "前进 1 秒",
  "Seek backward 1 second": "后退 1 秒",
  "Next frame": "下一帧",
  "Previous frame": "上一帧",
  "Jump forward 5 seconds": "前进 5 秒",
  "Jump backward 5 seconds": "后退 5 秒",
  "Go to timeline start": "转到时间线起点",
  "Go to timeline end": "转到时间线终点",
  "Split elements at playhead": "在播放头处分割元素",
  "Split and remove left": "分割并移除左侧",
  "Split and remove right": "分割并移除右侧",
  "Delete current selection": "删除当前选择",
  "Copy selected elements": "复制所选元素",
  "Paste elements at playhead": "在播放头处粘贴元素",
  "Duplicate selected elements": "创建所选元素的副本",
  "Toggle snapping": "开关吸附",
  "Toggle ripple editing": "开关波纹编辑",
  "Extract or recover source audio": "分离或恢复源音频",
  "Select all elements": "选择全部元素",
  "Deselect all elements": "取消选择全部元素",
  "Cancel current interaction": "取消当前交互",
  "Mute / unmute selected elements": "静音 / 取消静音所选元素",
  "Show / hide selected elements": "显示 / 隐藏所选元素",
  "Toggle bookmark at playhead": "切换播放头处的书签",
  Undo: "撤销",
  Redo: "重做",
  "Remove selected media asset": "移除所选素材",
  "Remove selected media assets": "移除多个所选素材",
  "Zoom timeline in": "放大时间线",
  "Zoom timeline out": "缩小时间线",
  "Zoom to selection": "缩放至所选元素",
  "Search shortcuts": "搜索快捷键",
  Action: "操作",
  Keys: "按键",
  Scope: "作用范围",
  Workspace: "工作区",
  Unassigned: "未分配",
  Available: "可用",
  Enabled: "已开启",
  Disabled: "已关闭",
  Run: "执行",
  "Add shortcut": "添加快捷键",
  "Change {key}": "修改 {key}",
  "Clear {key}": "清除 {key}",
  "Reset action": "重置此操作",
  "Reset all shortcuts": "重置所有快捷键",
  "Restore default shortcuts?": "恢复默认快捷键？",
  "This replaces every custom binding. Project contents and undo history stay unchanged.":
    "这会替换所有自定义按键。工程内容和撤销历史保持不变。",
  "Restore defaults": "恢复默认",
  Cancel: "取消",
  "Save shortcut": "保存快捷键",
  "Record a shortcut": "录入快捷键",
  "Press a key combination. Escape cancels recording; Tab moves focus.":
    "按下组合键。Escape 取消录入；Tab 移动焦点。",
  "Shortcut conflict with {action}. Clear or change that action first.":
    "与“{action}”的快捷键冲突，请先清除或修改该操作。",
  "Shortcut conflict: {action}": "快捷键冲突：{action}",
  "No matching shortcuts.": "没有匹配的快捷键。",
  "Keys use physical keyboard positions, as in Classic. Command on macOS becomes Control on Windows and Linux.":
    "按键沿用 Classic 的物理键位。macOS 的 Command 对应 Windows 和 Linux 的 Control。",
  "Text fields, IME composition, dialogs and focused controls keep their own keys. Timeline and asset actions require focus in that panel.":
    "文本框、输入法组字、弹窗和已聚焦控件保留自己的按键。时间线和素材操作要求焦点位于对应面板。",
  "Hold navigation keys to repeat. Editing, playback toggles and history actions run once per press. Existing review and recovery steps still apply.":
    "按住导航键可连续操作。编辑、播放切换和历史操作每次按下只执行一次。既有确认与恢复步骤仍然适用。",
  "Escape also cancels local editing gestures. Some operating system shortcuts cannot be delivered to the browser.":
    "Escape 也用于取消局部编辑手势。部分系统快捷键不会传递给浏览器。",
  "Import / export shortcut settings": "导入 / 导出快捷键设置",
  "Shortcut configuration JSON": "快捷键配置 JSON",
  "Import configuration": "导入配置",
  "Show current configuration": "显示当前配置",
  "Copy configuration": "复制配置",
  "Configuration copied.": "配置已复制。",
  "Configuration imported.": "配置已导入。",
  "Clipboard access is unavailable. Select and copy the configuration text.":
    "无法访问系统剪贴板。请选中配置文本进行复制。",
  "Tab is reserved for moving keyboard focus.": "Tab 保留用于移动键盘焦点。",
  "Escape is reserved for cancelling interactions.":
    "Escape 保留用于取消交互。",
  "This shortcut is reserved by the browser or operating system.":
    "此快捷键由浏览器或操作系统保留。",
  "Choose a key with optional Command / Control, Alt and Shift modifiers.":
    "请选择按键，可搭配 Command / Control、Alt 和 Shift 修饰键。",
  "Each action supports up to four shortcuts.":
    "每项操作最多可配置四个快捷键。",
  "The shortcut configuration is too large.": "快捷键配置过大。",
  "The shortcut configuration is invalid.":
    "快捷键配置无效。",
  "The shortcut configuration has unknown or missing actions.":
    "快捷键配置包含未知操作或缺少操作。",
  "The shortcut configuration contains an invalid key.":
    "快捷键配置包含无效按键。",
  "The shortcut configuration contains conflicting keys.":
    "快捷键配置包含冲突按键。",
  "Shortcut preferences could not be read. Default keys are active; the saved configuration has not been overwritten.":
    "无法读取快捷键偏好。已启用默认按键，保存的配置未被覆盖。",
  "Shortcuts changed for this session only. Browser storage is unavailable.":
    "快捷键仅在当前会话生效，浏览器存储不可用。",
  "This action is not available in the current editor view.":
    "当前编辑器视图不提供此操作。",
  "This action is still in progress.": "此操作仍在执行中。",
  "This action is currently unavailable.": "此操作当前不可用。",
  "Open a project first.": "请先打开工程。",
  "Finish or recover the current edit first.": "请先完成或恢复当前编辑。",
  "Finish the current interaction first.": "请先结束当前交互。",
  "No undo is available.": "没有可撤销的操作。",
  "No redo is available.": "没有可重做的操作。",
  "Select elements on unlocked tracks.": "请选择未锁定轨道上的元素。",
  "Select audio or video elements with known audio.":
    "请选择音频或已确认含音频的视频元素。",
  "Pause playback before editing multiple audio elements.":
    "编辑多个音频元素前请先暂停播放。",
  "Select visual elements to change visibility.":
    "请选择可视元素以修改显示状态。",
  "Live scene preview is unavailable.": "实时场景预览不可用。",
};
export function useShortcutText() {
  const { language } = useLocale();
  return (message: string, values: Record<string, string | number> = {}) => {
    const translated =
      friendlyCopy[message] ? t(message) : language === "zh" ? (zh[message] ?? t(message)) : t(message);
    return translated.replace(/\{(\w+)\}/g, (match, key) =>
      String(values[key] ?? match),
    );
  };
}
