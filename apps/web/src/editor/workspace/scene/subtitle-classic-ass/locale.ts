import { friendlyCopy } from "../../../shared/friendly-copy";
import { useLocale } from "../../../shared/locale";
const zh: Record<string, string> = {
  "ASS import": "ASS 导入",
  "Classic compatibility": "Classic 兼容导入",
  "Strict static ASS": "严格静态 ASS",
  "Uses Classic styles and automatic wrapping. Review ignored features before saving.":
    "使用 Classic 样式与自动换行。保存前请检查被忽略的功能。",
  "ASS import review": "ASS 导入审阅",
  "{count} subtitle cues. {skipped} invalid or empty cues skipped.":
    "共 {count} 条字幕，跳过 {skipped} 条无效或空字幕。",
  "Save reviewed subtitles": "保存已审阅字幕",
};
const warnings: [RegExp, string][] = [
  [
    /^Stripped unsupported ASS inline override tags from (\d+) subtitle cue\(s\)\.$/,
    "已从 {count} 条字幕中移除 Classic 不支持的 ASS 行内覆盖标签。",
  ],
  [
    /^Ignored ASS event effects in (\d+) subtitle cue\(s\)\.$/,
    "已忽略 {count} 条字幕中的 ASS 事件效果。",
  ],
  [
    /^Fell back to default subtitle styling for (\d+) cue\(s\) that referenced missing ASS styles\.$/,
    "有 {count} 条字幕引用了缺失的 ASS 样式，已使用默认样式。",
  ],
  [
    /^Ignored (\d+) non-dialogue ASS event\(s\)\.$/,
    "已忽略 {count} 条非对话 ASS 事件。",
  ],
];
export function useClassicAssLocale() {
  const { language } = useLocale();
  const t = (message: string, values: Record<string, string | number> = {}) =>
    (friendlyCopy[message] ? friendlyCopy[message][language === "zh" ? 1 : 0] : language === "zh" ? (zh[message] ?? message) : message).replace(
      /\{(\w+)\}/g,
      (match, key) => String(values[key] ?? match),
    );
  const warning = (message: string) => {
    if (language !== "zh") return message;
    for (const [pattern, translated] of warnings) {
      const match = message.match(pattern);
      if (match) return translated.replace("{count}", match[1]);
    }
    if (
      message ===
      "Ignored unsupported ASS style features such as outline, shadow, rotation, or scaling."
    )
      return "已忽略 Classic 不支持的 ASS 描边、阴影、旋转或缩放等样式功能。";
    if (message.startsWith("Explicit font alias: "))
      return "显式字体映射：" + message.slice("Explicit font alias: ".length);
    return message;
  };
  return { t, warning };
}
