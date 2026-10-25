/** Presentation copy only. Internal contracts and stored history labels stay exact. */
export const friendlyCopy: Record<string, readonly [string, string]> = {
  "Hold Alt for options": ["Hold Alt for options", "按住 Alt 可设置更多选项"],

  "Render scene frame": ["Preview current frame", "预览当前画面"],
  "Preparing scene frame…": ["Preparing picture…", "正在准备画面…"],
  "Use two normal-speed clips with enough length. Shorten the transition or restore normal speed.": ["Use two normal-speed clips with enough length. Shorten the transition or restore normal speed.", "请使用长度足够的正常速度片段。可缩短转场，或先将片段恢复为正常速度。"],

  "Picture preview is unavailable; audio continues.": ["Picture preview is unavailable; audio continues.", "画面暂时无法预览，声音继续播放。"],

  "Recover original edit": [
    "Check the last save",
    "确认上次保存"
  ],
  "Original edit saved.": [
    "Your last change was saved.",
    "上次修改已保存。"
  ],
  "Reading original edit recovery…": [
    "Checking the last save…",
    "正在确认上次保存结果…"
  ],
  "The original edit needs recovery before another change.": [
    "Check the last save before making more changes.",
    "请先确认上次保存结果，再继续编辑。"
  ],
  "Retry sends the original request and key. The project head is not an edit receipt.": [
    "Retrying checks the same change and will not save it twice.",
    "重试会继续处理上次修改，不会重复保存。"
  ],
  "Refresh recovery": [
    "Check again",
    "再次检查"
  ],
  "Repair recovery pointer": [
    "Find the last save",
    "查找上次保存"
  ],
  "Retry original edit": [
    "Retry saving",
    "重试保存"
  ],
  "Release resolved edit": [
    "Confirm result",
    "确认处理结果"
  ],
  "A readable nonempty local OPENCUT_WHISPER_MODEL is required": [
    "Speech recognition is not set up on this computer. Add a local speech model to use automatic subtitles.",
    "此电脑尚未配置语音识别模型，配置后即可自动生成字幕。"
  ],
  "Font size (Classic)": [
    "Font size relative to canvas",
    "随画布变化的字号"
  ],
  "Background horizontal padding (Classic size 15)": [
    "Horizontal text padding",
    "文字左右留白"
  ],
  "Background vertical padding (Classic size 15)": [
    "Vertical text padding",
    "文字上下留白"
  ],
  "Retry original property request": [
    "Retry saving changes",
    "重试保存修改"
  ],
  "Retry original keyframe request": [
    "Retry saving keyframes",
    "重试保存关键帧"
  ],
  "Resolve the original caption edit": [
    "Check the last subtitle save",
    "确认上次字幕保存"
  ],
  "The original receipt is saved. Acknowledge it to continue.": [
    "Your subtitles were saved. Confirm to continue.",
    "字幕已保存，确认后可继续。"
  ],
  "Retry the saved original request without changing its revision or key.": [
    "Retry the last subtitle save without creating duplicates.",
    "重试上次字幕保存，不会重复创建字幕。"
  ],
  "Pinned font": [
    "Subtitle font",
    "字幕字体"
  ],
  "Legacy captions": [
    "Existing subtitle track",
    "已有字幕轨道"
  ],
  "The project head is not a receipt. Retry sends the original transaction and key; it does not create a new plan.": [
    "Retrying checks the same change and will not save it twice.",
    "重试会继续处理上次修改，不会重复保存。"
  ],
  "Keys use physical keyboard positions, as in Classic. Command on macOS becomes Control on Windows and Linux.": [
    "Shortcuts follow key positions on your keyboard. Use Command on Mac and Control on Windows or Linux.",
    "快捷键按键盘位置识别。Mac 使用 Command，Windows 和 Linux 使用 Control。"
  ],
  "Text fields, IME composition, dialogs and focused controls keep their own keys. Timeline and asset actions require focus in that panel.": [
    "Click the timeline or media panel before using its shortcuts. Typing and open dialogs keep their usual keyboard behavior.",
    "使用时间线或素材快捷键前，请先点击对应面板。输入文字或打开弹窗时，按键保持原有用途。"
  ],
  "Hold navigation keys to repeat. Editing, playback toggles and history actions run once per press. Existing review and recovery steps still apply.": [
    "Hold navigation keys to keep moving. Editing and playback actions run once per press.",
    "按住方向快捷键可连续移动。编辑和播放操作每按一次执行一次。"
  ],
  "Escape also cancels local editing gestures. Some operating system shortcuts cannot be delivered to the browser.": [
    "Press Escape to cancel the current drag or edit. Some shortcuts are reserved by your system.",
    "按 Escape 取消当前拖动或编辑。部分快捷键由系统占用。"
  ],
  "Scope": [
    "Works in",
    "适用位置"
  ],
  "Release to review. Escape cancels without saving.": [
    "Release to move. Press Escape to cancel.",
    "松开鼠标移动片段，按 Escape 取消。"
  ],
  "Animation channels are preserved. These values are the base values used before animation.": [
    "Values with keyframes change during playback. These are their starting values.",
    "设有关键帧的属性会在播放时变化，这里显示基础值。"
  ],
  "This service does not support the required audio sample rates.": [
    "Audio export is unavailable on this computer.",
    "此电脑暂时无法导出音频。"
  ],
  "Audio WAV export currently requires Classic scene settings.": [
    "Convert this project from the project library before exporting audio.",
    "请先在工程列表转换此工程，再导出音频。"
  ],
  "Animation channels": [
    "Animated properties",
    "动画属性"
  ],
  "Animation channel": [
    "Animated property",
    "动画属性"
  ],
  "New animation channel": [
    "Animate another property",
    "添加动画属性"
  ],
  "Animation target": [
    "Apply animation to",
    "动画对象"
  ],
  "Element": [
    "Clip",
    "片段"
  ],
  "Channel value type": [
    "Value type",
    "数值类型"
  ],
  "Scalar property": [
    "Number",
    "数值"
  ],
  "Color component (linear RGB)": [
    "Color channel",
    "颜色通道"
  ],
  "Discrete stroke alignment": [
    "Stroke position",
    "描边位置"
  ],
  "Color parameter": [
    "Color to animate",
    "要调整的颜色"
  ],
  "Discrete parameter": [
    "Stroke to animate",
    "要调整的描边"
  ],
  "Animated parameter": [
    "Property",
    "属性"
  ],
  "Create channel with keyframe": [
    "Add animated property",
    "添加动画属性"
  ],
  "Keyframe time (seconds)": [
    "Time (seconds)",
    "时间（秒）"
  ],
  "Keyframe value": [
    "Value",
    "数值"
  ],
  "Set selected values": [
    "Update selected keyframes",
    "更新选中关键帧"
  ],
  "Discrete keyframe value": [
    "Stroke position",
    "描边位置"
  ],
  "View start (seconds)": [
    "Visible range starts at (seconds)",
    "显示起点（秒）"
  ],
  "View span (seconds)": [
    "Visible duration (seconds)",
    "显示时长（秒）"
  ],
  "Move selected by (seconds)": [
    "Move by (seconds)",
    "移动时间（秒）"
  ],
  "Paste keyframes into element": [
    "Paste keyframes into this clip",
    "粘贴到此片段"
  ],
  "Paste into selected channel": [
    "Paste into this property",
    "粘贴到此属性"
  ],
  "Curve preset": [
    "Animation easing",
    "动画缓动"
  ],
  "Flat curve reference span": [
    "Value range for equal endpoints",
    "起止值相同时的变化幅度"
  ],
  "Apply curve to selected segment": [
    "Apply between selected keyframes",
    "应用到选中关键帧之间"
  ],
  "Custom curve name": [
    "Preset name",
    "预设名称"
  ],
  "Save custom curve": [
    "Save preset",
    "保存预设"
  ],
  "Reset custom curves": [
    "Remove custom presets",
    "删除自定义预设"
  ],
  "Times are local to the element. Track channels use scene time. Dragging and pasting reject collisions and times outside the owner; no keys are silently replaced.": [
    "Clip keyframe times start from the beginning of the clip. Track keyframe times start from the beginning of the video. Keyframes cannot overlap or move outside their clip or track.",
    "片段关键帧的时间从片段开头计算，轨道关键帧从视频开头计算。关键帧不能重叠，也不能移出所在片段或轨道。"
  ],
  "Element properties": [
    "Clip properties",
    "片段属性"
  ],
  "Audio gain unit": [
    "Volume unit",
    "音量单位"
  ],
  "Audio gain": [
    "Volume",
    "音量"
  ],
  "Linear amplitude": [
    "Multiplier",
    "倍数"
  ],
  "Mute element": [
    "Mute clip",
    "静音此片段"
  ],
  "Save element properties": [
    "Save clip properties",
    "保存片段属性"
  ],
  "Animation channels are preserved. Animated parameters may override the base values shown here.": [
    "Values with keyframes change during playback. These are their base values.",
    "设有关键帧的属性会在播放时变化，这里显示基础值。"
  ],
  "Canvas and frame rate": [
    "Video size and frame rate",
    "视频尺寸与帧率"
  ],
  "Canvas preset": [
    "Size preset",
    "尺寸预设"
  ],
  "Choose dimensions": [
    "Choose a size",
    "选择尺寸"
  ],
  "Canvas width": [
    "Width (px)",
    "宽度（像素）"
  ],
  "Canvas height": [
    "Height (px)",
    "高度（像素）"
  ],
  "Restore last custom size": [
    "Use previous custom size",
    "使用上次自定义尺寸"
  ],
  "Frame-rate numerator": [
    "Frames",
    "帧数"
  ],
  "Frame-rate denominator": [
    "Duration (seconds)",
    "对应时长（秒）"
  ],
  "Blur main track": [
    "Blurred video background",
    "模糊视频背景"
  ],
  "Background blur intensity": [
    "Background blur",
    "背景模糊强度"
  ],
  "Save canvas and frame rate": [
    "Save video settings",
    "保存视频设置"
  ],
  "Frame-rate changes preserve exact authored times. Playback and export use the saved frame rate.": [
    "Changing the frame rate keeps clips and keyframes at their current times. Playback and export use the new frame rate.",
    "更改帧率不会移动片段和关键帧的时间位置。播放和导出将使用新帧率。"
  ],
  "Retry original captions": [
    "Retry saving subtitles",
    "重试保存字幕"
  ],
  "Acknowledge original result": [
    "Confirm result",
    "确认处理结果"
  ],
  "New editable subtitle track": [
    "New subtitle track",
    "新建字幕轨道"
  ],
  "Local transcription ready": [
    "Speech recognition is ready",
    "语音识别已就绪"
  ],
  "Local transcription unavailable": [
    "Speech recognition is unavailable",
    "语音识别暂不可用"
  ],
  "Checking local transcription…": [
    "Checking speech recognition…",
    "正在检查语音识别…"
  ],
  "Transcribe saved scene": [
    "Recognize speech",
    "识别语音"
  ],
  "Retry original transcription": [
    "Retry speech recognition",
    "重试语音识别"
  ],
  "Scene transcription": [
    "Speech recognition result",
    "语音识别结果"
  ],
  "Choose a saved task": [
    "Choose a result",
    "选择识别结果"
  ],
  "This transcript belongs to another saved revision. Transcribe the current scene before generating captions.": [
    "The video has changed since this result was created. Recognize speech again before generating subtitles.",
    "视频内容已更改，请重新识别语音后生成字幕。"
  ],
  "Inspect original caption recovery": [
    "Check the last subtitle save",
    "检查上次字幕保存"
  ],
  "Import a project font in Text before creating captions.": [
    "Import a font in Text before creating subtitles.",
    "请先在“文字”面板导入字体，再创建字幕。"
  ],
  "Map ASS font": [
    "Replacement font for",
    "替换字体"
  ],
  "Choose an imported face": [
    "Choose an imported font",
    "选择已导入字体"
  ],
  "ASS styles are preserved where supported. Unsupported ASS features are rejected. Only explicit q2 line breaks are currently supported.": [
    "This mode preserves supported subtitle styles and explicit line breaks. Remove unsupported formatting before importing.",
    "此模式保留支持的字幕样式，仅支持手动换行。请先移除不支持的格式，再导入。"
  ],
  "Font aliases change the original ASS font family and may change glyph layout. The saved import will report this substitution.": [
    "Replacing a font may change the text layout. The import result will list any replacement fonts.",
    "替换字体可能改变文字排版，导入结果会记录所用的替换字体。"
  ],
  "Shortcut configuration JSON": [
    "Shortcut settings",
    "快捷键设置内容"
  ],
  "Split elements at playhead": [
    "Split clips at playhead",
    "在播放位置分割片段"
  ],
  "Copy selected elements": [
    "Copy selected clips",
    "复制选中片段"
  ],
  "Paste elements at playhead": [
    "Paste clips at playhead",
    "在播放位置粘贴片段"
  ],
  "Duplicate selected elements": [
    "Duplicate selected clips",
    "创建选中片段的副本"
  ],
  "Select all elements": [
    "Select all clips",
    "选择全部片段"
  ],
  "Deselect all elements": [
    "Deselect all clips",
    "取消选择全部片段"
  ],
  "Mute / unmute selected elements": [
    "Mute / unmute selected clips",
    "静音或取消静音选中片段"
  ],
  "Show / hide selected elements": [
    "Show / hide selected clips",
    "显示或隐藏选中片段"
  ],
  "Toggle bookmark at playhead": [
    "Add marker",
    "打点"
  ],
  "Gain keyframes": [
    "Volume keyframes",
    "音量关键帧"
  ],
  "Gain keyframe": [
    "Volume keyframe",
    "音量关键帧"
  ],
  "Volume automation": [
    "Volume changes",
    "音量变化"
  ],
  "Retry original brand acquisition": [
    "Retry importing brand asset",
    "重试导入品牌素材"
  ],
  "Retry original acquisition": [
    "Retry importing asset",
    "重试导入素材"
  ],
  "Acquire brand": [
    "Import brand asset",
    "导入品牌素材"
  ],
  "Visual stack": [
    "Effects and masks",
    "效果与蒙版"
  ],
  "Edit visual stack": [
    "Edit effects and masks",
    "编辑效果与蒙版"
  ],
  "Edit stack": [
    "Edit effects and masks",
    "编辑效果与蒙版"
  ],
  "Visual-stack authoring is unavailable on this host.": [
    "Editing effects and masks is unavailable in this environment.",
    "当前环境暂不支持编辑效果与蒙版。"
  ],
  "Classic Canvas italic synthesis": [
    "Simulated italic",
    "合成斜体"
  ],
  "Reading font synthesis proof…": [
    "Checking font style support…",
    "正在检查字体样式支持…"
  ],
  "Classic height / 90": [
    "Scale with canvas height",
    "随画布高度缩放"
  ],
  "Classic compatibility": [
    "Compatibility mode",
    "兼容模式"
  ],
  "Strict static ASS": [
    "Strict style preservation",
    "严格保留样式"
  ],
  "Loading locked font…": [
    "Loading project font…",
    "正在加载工程字体…"
  ],
  "Scene effect layers": [
    "Video effect layers",
    "视频效果图层"
  ],
  "Native render tools differ or were not locked. Exact reproduction is not certified.": [
    "The export environment differs from the saved one, or its details are incomplete. The resulting picture and sound may differ.",
    "当前导出环境与保存时不同，或未记录完整环境信息，输出画面和声音可能存在差异。"
  ],
  "Retry sends the same transaction and key. The shared recovery panel retains an uncertain edit across refresh.": [
    "Retrying continues the same change without saving it twice. You can still check the result after refreshing the page.",
    "重试会继续处理上次修改，不会重复保存。刷新页面后仍可检查保存结果。"
  ],
  "Enter a finite value.": [
    "Enter a valid number.",
    "请输入有效数值。"
  ],
  "Enter a finite keyframe value.": [
    "Enter a valid keyframe value.",
    "请输入有效的关键帧数值。"
  ],
  "Enter a finite property value.": [
    "Enter a valid property value.",
    "请输入有效的属性值。"
  ],
  "Enter a positive 32-bit integer.": [
    "Enter a valid positive whole number.",
    "请输入有效的正整数。"
  ],
  "Enter positive 32-bit dimensions and frame-rate values.": [
    "Enter valid positive whole numbers for the dimensions and frame rate.",
    "请输入有效的正整数尺寸和帧率。"
  ],
  "Select two adjacent scalar keys in this channel.": [
    "Select two neighboring numeric keyframes for this property.",
    "请选择同一属性中相邻的两个数值关键帧。"
  ],
  "Enter a finite flat-curve reference span.": [
    "Enter a valid value range.",
    "请输入有效的变化幅度。"
  ],
  "Select a single clipboard channel for this destination.": [
    "Copy keyframes from one property before pasting here.",
    "请只复制一个属性的关键帧，再粘贴到此处。"
  ],
  "position_x_px": [
    "Horizontal position",
    "水平位置"
  ],
  "position_y_px": [
    "Vertical position",
    "垂直位置"
  ],
  "scale_x": [
    "Horizontal scale",
    "水平缩放"
  ],
  "scale_y": [
    "Vertical scale",
    "垂直缩放"
  ],
  "rotation_degrees": [
    "Rotation",
    "旋转"
  ],
  "opacity": [
    "Opacity",
    "不透明度"
  ],
  "gain_decibels": [
    "Volume (dB)",
    "音量（dB）"
  ],
  "gain_linear_legacy": [
    "Volume multiplier",
    "音量倍数"
  ],
  "font_size_classic_height90": [
    "Font size relative to canvas",
    "随画布变化的字号"
  ],
  "font_size_canvas_pixels": [
    "Font size (px)",
    "字号（像素）"
  ],
  "letter_spacing_px": [
    "Letter spacing",
    "字间距"
  ],
  "line_height_multiplier": [
    "Line spacing",
    "行间距"
  ],
  "text_background_padding_x_at_size15": [
    "Horizontal text padding",
    "文字左右留白"
  ],
  "text_background_padding_y_at_size15": [
    "Vertical text padding",
    "文字上下留白"
  ],
  "text_background_offset_x_px": [
    "Background horizontal offset",
    "背景水平偏移"
  ],
  "text_background_offset_y_px": [
    "Background vertical offset",
    "背景垂直偏移"
  ],
  "text_background_corner_radius_percent": [
    "Background roundness",
    "背景圆角"
  ],
  "graphic_stroke_width_px": [
    "Outline width",
    "描边宽度"
  ],
  "graphic_corner_radius_percent": [
    "Corner roundness",
    "圆角"
  ],
  "graphic_sides": [
    "Sides",
    "边数"
  ],
  "graphic_points": [
    "Points",
    "尖角数"
  ],
  "graphic_depth_percent": [
    "Inner depth",
    "内凹深度"
  ],
  "classic_blur_intensity": [
    "Blur amount",
    "模糊强度"
  ],
  "legacy_blur_radius_px": [
    "Blur radius",
    "模糊半径"
  ],
  "legacy_exposure_ev": [
    "Exposure",
    "曝光"
  ],
  "legacy_contrast": [
    "Contrast",
    "对比度"
  ],
  "legacy_saturation": [
    "Saturation",
    "饱和度"
  ],
  "mask_center_x": [
    "Mask horizontal position",
    "蒙版水平位置"
  ],
  "mask_center_y": [
    "Mask vertical position",
    "蒙版垂直位置"
  ],
  "mask_width": [
    "Mask width",
    "蒙版宽度"
  ],
  "mask_height": [
    "Mask height",
    "蒙版高度"
  ],
  "mask_rotation_degrees": [
    "Mask rotation",
    "蒙版旋转"
  ],
  "mask_scale": [
    "Mask scale",
    "蒙版缩放"
  ],
  "mask_feather_classic_pixels": [
    "Feather (px)",
    "羽化（像素）"
  ],
  "mask_feather_legacy_canvas_fraction": [
    "Feather relative to canvas",
    "随画布变化的羽化"
  ],
  "mask_stroke_width_px": [
    "Mask outline width",
    "蒙版描边宽度"
  ],
  "mask_font_size_classic_height90": [
    "Mask font size relative to canvas",
    "随画布变化的蒙版字号"
  ],
  "mask_font_size_canvas_pixels": [
    "Mask font size (px)",
    "蒙版字号（像素）"
  ],
  "mask_letter_spacing_px": [
    "Mask letter spacing",
    "蒙版文字字间距"
  ],
  "mask_line_height_multiplier": [
    "Mask line spacing",
    "蒙版文字行间距"
  ],
  "text_color": [
    "Text color",
    "文字颜色"
  ],
  "text_background_color": [
    "Text background",
    "文字背景"
  ],
  "graphic_fill": [
    "Shape fill",
    "形状填充"
  ],
  "graphic_stroke": [
    "Shape outline",
    "形状描边"
  ],
  "mask_stroke": [
    "Mask outline",
    "蒙版描边"
  ],
  "graphic_stroke_align": [
    "Shape stroke position",
    "形状描边位置"
  ],
  "mask_stroke_align": [
    "Mask stroke position",
    "蒙版描边位置"
  ],
  "Save stack": [
    "Save effects and masks",
    "保存效果与蒙版"
  ],
  "Saving stack…": [
    "Saving effects and masks…",
    "正在保存效果与蒙版…"
  ],
  "This stack uses a legacy visual profile.": [
    "This clip uses older effect settings.",
    "此片段使用较早版本的效果设置。"
  ],
  "Select a visual element to edit its stack.": [
    "Select a clip to edit its effects and masks.",
    "选择片段后即可编辑效果与蒙版。"
  ],
  "This mask uses a legacy profile. Its exact fields are preserved; only order, enabled and removal are editable here.": [
    "This mask was created with older settings. You can reorder it, turn it on or off, or remove it.",
    "此蒙版使用较早版本的设置。目前可以调整顺序、启用、停用或删除。"
  ],
  "Stroke and text-mask rendering require the corresponding Scene renderer capability.": [
    "Some environments may not support outlines or text masks.",
    "部分运行环境可能不支持描边或文字蒙版。"
  ],
  "Base values are preserved alongside keyframes. Static canvas handles are unavailable for animated targets.": [
    "This clip has animation. Use its keyframes to adjust the animated properties.",
    "此片段包含动画，请在关键帧中调整动画属性。"
  ],
  "Finish playback or resolve the current edit before editing a stack.": [
    "Pause playback or finish the current edit before changing effects and masks.",
    "请先暂停播放或完成当前编辑，再调整效果与蒙版。"
  ],
  "Move volume automation": [
    "Move volume curve",
    "移动音量曲线"
  ],
  "Sampling volume curve…": [
    "Loading volume curve…",
    "正在加载音量曲线…"
  ],
  "Automation editing is unavailable on this host.": [
    "Volume animation is unavailable in this environment.",
    "当前环境暂不支持编辑音量动画。"
  ],
  "Canvas transforms are unavailable on this host.": [
    "Adjusting clips on the canvas is unavailable in this environment.",
    "当前环境暂不支持直接在画布上调整片段。"
  ],
  "Draft keys; the curve still shows the saved revision.": [
    "Unsaved keyframes. The curve shows your last saved changes.",
    "关键帧尚未保存，曲线显示上次保存的内容。"
  ]
};
