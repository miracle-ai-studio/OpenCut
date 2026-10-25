import { friendlyCopy } from "../../../shared/friendly-copy";
import { t, useLocale } from "../../../shared/locale";
const zh: Record<string, string> = {
  "Volume curve": "音量曲线",
  "Volume point": "音量控制点",
  "Curve view": "曲线显示范围",
  "Whole clip": "全段",
  Opening: "开头",
  Ending: "结尾",
  "control points": "个控制点",
  "Smooth changes": "平滑变化",
  "Straight changes": "直线变化",
  "Drag a point to adjust it. Hold Alt for fine control. Use Smooth changes for a gentle fade.":
    "拖动圆点可调整音量和时间，按住 Alt 可细调；选择「平滑变化」可让渐入渐出更柔和。",
  "Volume automation": "音量自动化",
  "Gain keyframes": "音量关键帧",
  "Gain keyframe": "音量关键帧点",
  "Move volume automation": "移动音量自动化曲线",
  "Delete selected keys": "删除所选关键帧",
  "Add key at playhead": "在播放头添加关键帧",
  "Sampling volume curve…": "正在采样音量曲线…",
  "Saving volume keys…": "正在保存音量关键帧…",
  "The gain target changed. Reopen its controls before editing.":
    "音量目标已变化，请重新打开控件。",
  "The sampled gain curve differs from the fixed channel.":
    "采样音量曲线与固定通道不一致。",
  "The gain-keyframe plan differs from the original gesture.":
    "音量关键帧规划与原始手势不一致。",
  "Drag a key to change time and value. Shift-click selects more keys; drag the curve to offset all keys.":
    "拖动关键帧可更改时间和值；Shift 点击多选；拖动曲线可整体调整所有关键帧。",
  "Release to save volume changes.": "松开即可保存音量变化。",
  "Draft keys; the curve still shows the saved revision.":
    "正在编辑关键帧；曲线仍显示已保存修订。",
  "Automation editing is unavailable on this host.":
    "此主机尚未提供音量自动化编辑。",
  "Resolve the current edit or pause playback before editing gain keys.":
    "请先处理未完成的编辑或暂停播放，再编辑音量关键帧。",
  "Empty or duplicate gain samples are invalid.": "音量采样为空或重复。",
  "Volume automation has too many visible keys for this editor.":
    "当前音量通道的关键帧过多，请使用关键帧编辑器。",
  "Convert the historical curve before inserting a control point":
    "这条曲线来自旧工程。先选择平滑变化或直线变化，再添加控制点。",
  Cancel: "取消",
};
export function useGainText() {
  const { language } = useLocale();
  return (s: string) =>
    friendlyCopy[s] ? t(s) : language === "zh" ? (zh[s] ?? t(s)) : t(s);
}
