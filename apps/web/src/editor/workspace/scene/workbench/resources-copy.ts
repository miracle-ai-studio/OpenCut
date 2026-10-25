import { useLocale, type Language } from "../../../shared/locale";
const copy = {
  sourceContext: ["Source media", "源素材"],
  effectContext: ["Effect example", "效果样例"],
  timelineContext: ["Timeline", "时间线"],
  returnTimeline: ["Return to timeline", "返回时间线"],
  sourceIn: ["In point (seconds)", "入点（秒）"],
  sourceOut: ["Out point (seconds)", "出点（秒）"],
  markIn: ["Mark in · I", "标记入点 · I"],
  markOut: ["Mark out · O", "标记出点 · O"],
  sourceSelected: ["Selected range", "选段"],
  sourceAdd: ["Add selected range", "添加选段"],
  sourceTarget: ["Target track", "目标轨道"],
  sourceNewTrack: ["Create a new track", "将创建新轨道"],
  sourceRangeInvalid: ["Use 0 ≤ In < Out ≤ source duration.", "请满足 0 ≤ 入点 < 出点 ≤ 源素材时长。"],
  sourceRangeConfirm: ["Enter to confirm · Escape to restore", "Enter 确认 · Esc 还原"],
  sourceDurationUnavailable: ["Source duration is not confirmed. Range placement is unavailable.", "尚未确认源素材时长，暂不能选择或添加范围。"],
  previewUnconfirmed: ["Resolve the original audio session before starting another preview.", "请先核对原声音会话，再开始其它预览。"],
  previewPauseFailed: ["Could not confirm paused timeline playback. Retry before playing this source.", "尚未确认时间线已暂停，请重试后再播放源素材。"],
  tabMedia: ["Media", "素材"],
  tabAudio: ["Audio", "音频"],
  tabText: ["Text", "文本"],
  tabGraphic: ["Graphics", "图形"],
  tabEffect: ["Effects", "特效"],
  tabTransition: ["Transitions", "转场"],
  tabCaption: ["Captions", "字幕"],
  preview: ["Preview", "预览"],
  original: ["Original", "原画"],
  effectExample: ["Effect example", "特效示例"],
  effectKeyboard: [
    "Enter to preview · Shift+Enter to add to the selected clip",
    "Enter 预览 · Shift+Enter 添加到所选片段",
  ],
  dropStart: ["Start", "起点"],
  dropEnd: ["End", "终点"],
  dropDuration: ["Duration", "时长"],
  dropNewTrack: ["New track", "将创建新轨道"],
  dropDurationPending: [
    "Duration confirmed after acquisition",
    "时长将在获取后确认",
  ],
  dropAttach: ["Attach to clip", "附着到片段"],
  dropTransition: ["Add transition", "添加转场"],
  dropReplace: ["Replace transition", "替换转场"],
  sample: [
    "Example only · apply to see it on your clips",
    "效果示例 · 添加后可在画面中查看实际效果",
  ],
  blur: ["Blur", "模糊"],
  soft: ["Soft", "轻柔"],
  medium: ["Medium", "适中"],
  strong: ["Strong", "明显"],
  applyClip: ["Add to selected clip", "添加到所选片段"],
  applyLayer: ["Add effect layer", "添加效果图层"],
  chooseClip: [
    "Select an unlocked visual clip to add this effect to it.",
    "选择未锁定的画面片段，即可为它添加效果。",
  ],
  effectLayerHint: [
    "An effect layer affects the visual layers below it from the playhead.",
    "效果图层从播放头位置开始，影响它下方的画面。",
  ],
  layerDuration: ["Effect duration (seconds)", "效果时长（秒）"],
  appliedEffects: ["Effects on this timeline", "时间线中的效果"],
  quick: ["Quick · 0.25 s", "快速 · 0.25 秒"],
  smooth: ["Smooth · 0.5 s", "平滑 · 0.5 秒"],
  slow: ["Slow · 1 s", "舒缓 · 1 秒"],
  presets: ["Presets", "预设"],
  playExample: ["Play example", "播放示例"],
  stopExample: ["Stop example", "停止示例"],
  before: ["Before", "前一画面"],
  after: ["After", "后一画面"],
  updateTransition: ["Update transition", "更新转场"],
  transitionUpdated: [
    "Transition saved. Play the timeline to see it on your clips.",
    "转场已保存。播放时间线即可查看片段的实际效果。",
  ],
  effectSaved: [
    "Effect saved. View the result in the preview.",
    "效果已保存，可在预览画面中查看。",
  ],
  positiveDuration: [
    "Enter a duration greater than zero.",
    "请输入大于零的时长。",
  ],
  add: ["Add to timeline", "添加到时间线"],
  addSettings: ["Placement settings", "添加位置设置"],
  placementHint: [
    "Choose where to add this clip. You can adjust it on the timeline afterwards.",
    "选择片段的添加位置，添加后仍可在时间线上调整。",
  ],
  saved: ["Added to timeline", "已添加到时间线"],
  addedProject: [
    "Added to project. Choose Add to timeline to place it.",
    "已添加到工程，点击“添加到时间线”即可使用。",
  ],
  saving: ["Saving media…", "正在保存素材…"],
  recover: ["Continue saving", "继续保存"],
  recoveryTitle: ["Finish saving media", "完成素材保存"],
  retry: ["Retry original save", "重试原保存操作"],
  discard: ["Cancel this addition", "取消本次添加"],
  changed: [
    "The project or scene changed. Return to the original scene to continue.",
    "工程或场景已改变，请返回原场景后继续。",
  ],
  libraryHint: [
    "Listen first, then add a sound at the playhead.",
    "先试听，再将音效添加到播放头位置。",
  ],
  listen: ["Listen", "试听"],
  pause: ["Pause audition", "暂停试听"],
  cancelPreview: ["Cancel audition", "取消试听"],
  retryPreview: ["Retry audition", "重试试听"],
  waveformOverview: ["Whole-sound waveform overview", "完整音效波形概览"],
  waveformUnavailable: ["Waveform unavailable", "波形暂不可用"],
  durationUnavailable: ["Duration is not available", "时长暂不可用"],
  auditionProgress: ["Audition progress", "试听进度"],
  stop: ["Stop", "停止"],
  loading: ["Loading sound…", "正在加载音效…"],
  previewFailed: [
    "Could not play this sound. Try again.",
    "暂时无法播放此音效，请重试。",
  ],
  createSound: ["Create a sound", "制作音效"],
  createHint: [
    "Make a short sound on this computer and add it to the timeline.",
    "在本机制作简短音效，并添加到时间线。",
  ],
  createAdd: ["Create and add", "制作并添加"],
  soundOptions: ["Sound settings", "音效设置"],
  variation: ["Sound variation", "音色变化"],
  sources: ["Source and license", "来源与许可"],
  projectSounds: ["Sounds in this project", "工程中的音效"],
  projectGraphics: ["Graphics in this project", "工程中的图形"],
  click: ["Click", "点击"],
  mouseclick: ["Mouse click", "鼠标按下"],
  mouserelease: ["Mouse release", "鼠标松开"],
  rollover: ["Hover", "轻触"],
  switch: ["Switch", "切换"],
  noMatches: [
    "No matching sounds. Try another search.",
    "没有找到匹配的音效，请换个关键词。",
  ],
} as const;
export type ResourceCopyKey = keyof typeof copy;
export function resourceCopy(key: ResourceCopyKey, language: Language): string {
  return copy[key][language === "zh" ? 1 : 0];
}
export function useResourceCopy() {
  const { language } = useLocale();
  return (key: ResourceCopyKey) => resourceCopy(key, language);
}
export function soundName(name: string, language: Language) {
  const match = /^(mouseclick|mouserelease|rollover|switch|click)(\d+)$/i.exec(
    name,
  );
  return match
    ? `${resourceCopy(match[1].toLowerCase() as ResourceCopyKey, language)} ${Number(match[2])}`
    : name;
}
