import { useLocale } from "../../../shared/locale";
const copy = {
  choose: [
    "Select a cut between clips on the timeline.",
    "请在时间线选择两个片段之间的接缝。",
  ],
  seam: ["Clip cut", "片段接缝"],
  target: ["Transition target", "转场目标"],
  replace: ["Replace transition", "替换转场"],
  replacement: [
    "This cut already has a transition. Applying replaces it in place.",
    "此接缝已有转场，应用将原位替换。",
  ],
  locked: [
    "Unlock this track before editing the transition.",
    "轨道已锁定，请解锁后编辑转场。",
  ],
  disabled: [
    "Enable both clips and their track first.",
    "请先启用两个片段及所在轨道。",
  ],
  videoOnly: [
    "Crossfade currently requires two video clips.",
    "当前叠化转场仅支持两个视频片段。",
  ],
  normalSpeed: [
    "Crossfade requires video clips at normal speed.",
    "叠化转场要求两个视频片段均为原速。",
  ],
  unknownDuration: [
    "Source duration is unavailable. Cannot validate transition handles.",
    "无法读取源素材时长，暂不能验证转场余量。",
  ],
  noHandles: [
    "Leave source footage after the first clip and before the second clip by trimming their source ranges.",
    "源素材余量不足。请调整源范围，为前片段尾部和后片段头部预留画面。",
  ],
  maximum: ["Maximum duration", "可用最大时长"],
  tooLong: [
    "Duration exceeds the clips or source handles.",
    "时长超出片段或源素材余量。",
  ],
  sourceNote: [
    "The transition is centered on this cut. Clip positions stay unchanged.",
    "转场以接缝为中心，两侧片段位置保持不变。",
  ],
  wait: ["Wait for the current edit to finish.", "请等待当前编辑完成。"],
  history: [
    "Return to the current revision before editing.",
    "请返回当前版本后编辑。",
  ],
  recovery: [
    "Resolve the original edit before starting another operation.",
    "请先处理原编辑的保存结果。",
  ],
  draft: [
    "Finish or cancel the current edit first.",
    "请先完成或取消当前编辑。",
  ],
  changed: [
    "The transition target changed. Select the cut and try again.",
    "转场目标已改变，请重新选择接缝后操作。",
  ],
  preview: ["Preview transition", "预览转场"],
  removed: ["Transition removed.", "转场已移除。"],
  close: ["Clear cut selection", "取消接缝选择"],
} as const;
export function useTransitionCopy() {
  const { language } = useLocale();
  return (key: keyof typeof copy) => copy[key][language === "zh" ? 1 : 0];
}
