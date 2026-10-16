import { t, useLocale } from "../../../shared/locale";
const zh: Record<string, string> = {
  "Reading font synthesis proof…": "正在读取字体合成证明…",
  "Mask canvas controls": "蒙版画布控件",
  "Move mask": "移动蒙版",
  "Resize mask width": "调整蒙版宽度",
  "Resize mask height": "调整蒙版高度",
  "Move incoming tangent": "移动入切线",
  "Move outgoing tangent": "移动出切线",
  "Move path point": "移动路径点",
  "Scale mask": "缩放蒙版",
  "Rotate mask": "旋转蒙版",
  "Adjust feather": "调整羽化",
  "Duration must be positive.": "时长必须大于零。",
  "Enter finite mask coordinates.": "请输入有限的蒙版坐标。",
  "Read the target geometry before adding a sized mask.":
    "添加定尺寸蒙版前，需要读取目标几何。",
  "Choose an imported static font before adding a text mask.":
    "添加文字蒙版前，请选择已导入的静态字体。",
  "Choose an imported static font before saving a text mask.":
    "保存文字蒙版前，请选择已导入的静态字体。",
  "Feather and stroke must be finite and nonnegative.":
    "羽化和描边必须是非负有限值。",
  "Color channels must be between zero and one.":
    "颜色通道必须在 0 至 1 之间。",
  "Enter positive mask dimensions and a nonnegative scale.":
    "蒙版尺寸必须大于零，缩放不能为负数。",
  "A closed path needs three points; point IDs must be unique and the limit is 4096.":
    "闭合路径至少需要三个点；点 ID 必须唯一，且最多 4096 个点。",
  "Enter finite path coordinates and a nonnegative scale.":
    "请输入有限路径坐标和非负缩放。",
  "Enter valid text mask size and spacing.": "请输入有效的文字蒙版字号与间距。",
  "Blur intensity must be finite and nonnegative.":
    "模糊强度必须是非负有限值。",
  "This stack uses a legacy visual profile.": "此堆栈使用旧版视觉语义。",
  "Unlock the effect track before reordering it.":
    "请先解锁效果轨道再调整顺序。",
  "Scene geometry differs from the requested fixed revision.":
    "场景几何与请求的固定修订不一致。",

  "Visual stack": "视觉堆栈",
  "Edit visual stack": "编辑视觉堆栈",
  "Save stack": "保存堆栈",
  "Saving stack…": "正在保存堆栈…",
  Masks: "蒙版",
  Effects: "效果",
  "Scene effect layers": "场景效果图层",
  "Add mask": "添加蒙版",
  "Mask shape": "蒙版形状",
  Rectangle: "矩形",
  Ellipse: "椭圆",
  Split: "分割",
  "Cinematic bars": "电影遮幅",
  Heart: "心形",
  Diamond: "菱形",
  Star: "星形",
  Freeform: "自由路径",
  Text: "文字",
  Enabled: "启用",
  Inverted: "反转",
  Earlier: "提前",
  Later: "延后",
  "Remove mask": "删除蒙版",
  "Remove effect": "删除效果",
  "Add blur": "添加模糊",
  "Blur intensity": "模糊强度",
  "Mask properties": "蒙版属性",
  "Feather (pixels)": "羽化（像素）",
  "Stroke width (pixels)": "描边宽度（像素）",
  "Stroke alignment": "描边对齐",
  Inside: "内侧",
  Center: "居中",
  Outside: "外侧",
  Red: "红",
  Green: "绿",
  Blue: "蓝",
  Alpha: "透明度",
  "X (%)": "X（%）",
  "Y (%)": "Y（%）",
  "Width (%)": "宽度（%）",
  "Height (%)": "高度（%）",
  "Rotation (degrees)": "旋转（度）",
  "Scale (%)": "缩放（%）",
  "Closed path": "闭合路径",
  "Path points": "路径点",
  "Point {index}": "点 {index}",
  "Anchor X (%)": "锚点 X（%）",
  "Anchor Y (%)": "锚点 Y（%）",
  "In X (%)": "入切线 X（%）",
  "In Y (%)": "入切线 Y（%）",
  "Out X (%)": "出切线 X（%）",
  "Out Y (%)": "出切线 Y（%）",
  "Insert after": "在后方插点",
  "Delete point": "删除点",
  "Append point": "追加点",
  "Imported font": "已导入字体",
  "Choose a font": "选择字体",
  Content: "内容",
  "Font size": "字号",
  "Font size unit": "字号单位",
  "Classic height / 90": "Classic 高度 / 90",
  "Canvas pixels": "画布像素",
  Decoration: "装饰",
  None: "无",
  Underline: "下划线",
  "Line through": "删除线",
  "Letter spacing (pixels)": "字距（像素）",
  "Line height": "行高",
  "Add effect layer": "添加效果图层",
  "Start (seconds)": "开始（秒）",
  "Duration (seconds)": "时长（秒）",
  "Remove layer": "删除图层",
  "Edit stack": "编辑堆栈",
  Cancel: "取消",
  "No masks.": "暂无蒙版。",
  "No effects.": "暂无效果。",
  "No scene effect layers.": "暂无场景效果图层。",
  "Select a visual element to edit its stack.": "选择视觉元素以编辑堆栈。",
  "Visual-stack authoring is unavailable on this host.":
    "此主机尚未提供视觉堆栈编辑。",
  "Base values are preserved alongside keyframes. Static canvas handles are unavailable for animated targets.":
    "基础值与关键帧同时保留。动画目标不可使用静态画布手柄。",
  "This mask uses a legacy profile. Its exact fields are preserved; only order, enabled and removal are editable here.":
    "此蒙版使用旧版语义。原始字段完整保留；此处仅可调整顺序、启用或删除。",
  "No imported static fonts are available. Import a font before creating a text mask.":
    "没有已导入的静态字体，请先导入字体再创建文字蒙版。",
  "Order follows compositing order, from earlier to later. An effect layer changes the layers before it.":
    "顺序表示从前到后的合成顺序；效果图层作用于排列在它之前的图层。",
  "Stroke and text-mask rendering require the corresponding Scene renderer capability.":
    "描边与文字蒙版需要对应的场景渲染能力。",
  "An open path is editable but does not mask the image until closed.":
    "开放路径可以编辑，闭合后才参与蒙版渲染。",
  "Finish playback or resolve the current edit before editing a stack.":
    "请先结束播放或处理当前未完成的编辑，再编辑堆栈。",
  "The stack target changed. Reopen its controls before editing.":
    "堆栈目标已变化，请重新打开编辑控件。",
  "Unlock the target track before editing its stack.": "请先解锁目标轨道。",
  "Remove this mask's keyframes before deleting it.":
    "请先删除此蒙版的关键帧。",
  "Remove this effect's keyframes before deleting it.":
    "请先删除此效果的关键帧。",
};
export function useSceneText() {
  const { language } = useLocale();
  return (text: string, values: Record<string, string | number> = {}) =>
    language === "zh" && zh[text]
      ? zh[text].replace(/\{(\w+)\}/g, (match, key) =>
          String(values[key] ?? match),
        )
      : t(text, values);
}
