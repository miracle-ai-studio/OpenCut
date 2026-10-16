import { useLocale, t } from "../../../shared/locale";
const zh: Record<string, string> = {
  "Pause the live preview before transforming picture content.":
    "请先暂停实时预览，再变换画面内容。",
  "Choose enabled, unlocked visual elements active at the current time.":
    "请选择当前时刻有效且已启用、未锁定的视觉元素。",
  "The canvas selection is not a unique supported group.":
    "画布选择必须是受支持且不重复的一组元素。",
  "Select visual elements before transforming them.":
    "请先选择要变换的视觉元素。",
  "Rotate nonuniformly scaled text separately; its Classic transform order cannot express a group rotation.":
    "请单独旋转非等比缩放的文字；其 Classic 变换顺序无法表示整组旋转。",
  "Select visible picture content": "选择可见画面内容",
  "Pause the live preview before selecting picture content.":
    "请先暂停实时预览，再选择画面内容。",
  "The pixel hit observation differs from the fixed canvas.":
    "像素命中观察与固定画布不一致。",
  "Selected element transform handles": "已选元素变换手柄",
  "Resize selected element": "调整已选元素大小",
  "Resize selected element horizontally": "横向调整已选元素大小",
  "Resize selected element vertically": "纵向调整已选元素大小",
  "This gesture would overwrite an animated position or rotation. Use the keyframe editor.":
    "本次手势会覆盖位置或旋转动画，请使用关键帧编辑器。",
  "Group transforms move member positions. Edit position-animated elements separately.":
    "整组变换会移动成员位置，请单独编辑已有位置动画的元素。",
  "Resizing removes animation on the resized scale axes. Other animation is preserved. Escape cancels without saving.":
    "调整大小会移除对应缩放轴的动画，其余动画保留。按 Escape 取消且不保存。",
  "The sampled transforms differ from the fixed target.":
    "采样变换与固定目标不一致。",
  "Sticker transforms require the sampled geometry capability.":
    "贴纸变换需要支持权威采样几何的主机。",
  "Move selected element": "移动已选元素",
  "Rotate selected element": "旋转已选元素",
  "Animated or transitioning elements require the keyframe editor.":
    "动画或转场中的元素需要使用关键帧编辑器。",
  "Transform-animated or transitioning elements require the keyframe editor.":
    "变换已有动画或处于转场中的元素需要使用关键帧编辑器。",
  "Canvas transforms are unavailable on this host.": "此主机尚未提供画布变换。",
  "Canvas geometry differs from the fixed target.":
    "画布几何与固定目标不一致。",
  "The transform plan differs from the original gesture.":
    "变换规划与原始手势不一致。",
  "Zero-sized elements cannot be edited on canvas.":
    "无法在画布上编辑零尺寸元素。",
};
export function useCanvasText() {
  const { language } = useLocale();
  return (s: string) => (language === "zh" ? (zh[s] ?? t(s)) : t(s));
}
