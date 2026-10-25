# 工程包选址与历史前后对比 · 2026-10-22

**两项已实现，并完成以下限定验收。本机 59790 服务已更新。** 这是前一轮 [UI 修复](ui-delivery-fixes-20261021.md)中工程包路径和对象级历史差异的后续；不替代其余 UI、渲染或整项迁移验收。

| Before：修复前 | After：当前行为 | Why：作用与边界 |
|---|---|---|
| 导出、恢复工程包需手填本机路径 | 点击“选择保存位置…”打开 macOS 保存窗口，选择名称和位置；恢复时选择工程包文件夹。取消不保存，已有目录拒绝覆盖，失败后可另选位置 | 保存位置可直接选择；继续使用已有 `.opencut` 工程包目录、校验和原子发布能力 |
| 历史主要是通用动作、操作者角色 | 显示记录中的具体账户和来源；展开后显示片段等对象、属性、修改前与修改后。恢复确认显示当前版本到目标版本的变化 | 可以判断谁修改了什么、是否应该恢复；没有记录的身份明确显示“操作者未记录”，不推测身份 |
| 数据库 UTC 时间被当作本地时间 | 无时区的数据库时间按 UTC 解析，按浏览器本地时区显示；已有时区偏移保留 | 上海时区跨日显示正确；列表和恢复确认使用同一格式 |

## 实现边界

选择位置使用唯一 Rust 类型操作 `project.package.location.choose`，叶操作为 `opencut_project_package_choose_location`。DTO → 工具注册/领域归属 → Application 权限与名称检查/端口 → Native 适配 → Host 装配 → 派生 SDK/OpenAPI → Web 均已接通。只有本机 owner 可打开窗口；选择本身不导出、不导入、不覆盖文件。直接 CLI 路径导出能力保留。

新增 `native-services::package_location` 模块只实现 Application 的 `PackageLocationPicker` 端口，macOS 系统窗口由固定 AppleScript 调用；动态文本通过 argv 传递。Domain/time/composition 不依赖系统窗口。窗口互斥、超时和子进程回收、返回值大小与路径验证均在适配边界。不增加 crate、外部依赖或 dependency-policy 例外；未修改 allowlist。

Web 继续复用原保存/检查/恢复请求、固定版本、幂等与恢复围栏。取消选址保留原选择及不确定的原请求；明确选择新位置后才清除旧尝试；连接切换后的迟到结果不应用。工程包仍是可编辑目录包，没有新增 zip 格式。

历史详情按需读取经过授权的两个固定公共版本和已有操作者记录，展示层计算差异；不复制提交、撤销或恢复的领域权威。覆盖片段、轨道、字幕、画布、音量、速度、效果、蒙版、动画、素材和字体等变化的友好名称。窄侧栏使用带“修改前/修改后”的卡片，恢复确认使用对照表。来源哈希、内部路径和协议标识不作为用户主信息。旧记录缺失的操作者不会补造。翻页也读取对应操作者，连接/工程/目标切换均隔离迟到回包。

## 验证结果

- 受影响 Web 集成检查 **5 文件 / 64 项通过**；最后只对时间与属性标签作修正，再跑历史相关 **3 文件 / 19 项通过**。两轮有重叠，不相加为新总数。工程包 34 项包含取消、换连接、原请求不确定及另选位置等流程。
- Rust `cargo test -j 2 -p opencut-editor-application -p opencut-native-services --lib location`：Application 1 项、Native 5 项通过。其中 3 项直接验证新增选址的权限/名称、取消/非法路径、Unicode/空格和已有目录保护，另外 3 项是过滤词匹配到的既有用例。
- 最终 TypeScript/Vite 构建通过，保留既有大包提示；Web 边界 **256 文件通过**。Python 架构 **14 项通过**，依赖检查 **22 模块 / 0 例外**，合同边界通过。
- SDK、工具/事件 fixture、OpenAPI、Web 类型从同一最终显式 Native 二进制生成，generated check 与 canonical check 通过：**184 操作 / 12 领域**。首次候选的新动作使用下划线，canonical 正确拒绝；修正为点分动作后重建并重新生成，未放宽门禁，原失败日志保留。
- 新 Native 的实际 MCP 发现包含选址动作；受限账户调用返回 `NOT_AUTHORIZED`，不打开系统窗口。CLI → HTTP 共用分派以非法名称验证 Application 拒绝路径。

真实状态验收复用已有 8 秒红绿/440 Hz 合成工程：本机账户通过 UI 将 `sample.mp4` 水平位置 **0 → 12.5 px**；具名“UI 验收助手 20261022”通过真实 MCP 将位置 **12.5 → 24 px**、音量 **0 → -6 dB**。随后撤销临时凭据、关闭其授权；历史仍显示原具名账户。中英文 × 明暗主题四组合均能读到对象与前后值，272 px 宽详情无横向溢出，零 `pageerror`。恢复确认对比当前和所选版本，取消不写入。

工程包浏览器流程仅替换系统选址返回值，其他调用使用真实服务：取消后不导出；已有目录拒绝且哨兵文件未变；另选位置后实际保存并校验；选择已保存包后真实检查成功，恢复按钮可用，未执行导入。首次验收最后关闭窗口时，测试定位器同时匹配关闭图标和页脚按钮而退出；没有产品错误。保留原日志，最终只读补查限定页脚按钮，成功关闭。

最后 Web 时间修正后，四组合在明确的 `Asia/Shanghai` 浏览器时区再次通过：数据库 `2026-10-21 16:43:38` 显示为 **2026/10/22 00:43:38**。复用原历史行和工程包，不重复提交编辑或导出；四张领域表完整摘要均不变。

## 数据、候选和可打开证据

本机服务复用 `http://127.0.0.1:59790` 和原 SQLite，Native 为 `069a21775bf41839ac2a2afa8172f228ff169fcd1e5254e776a6b0e193950869`。切换时四表不变；默认 8788 服务原本不就绪，未更改它。源码 HEAD 为 `abd2c969b4b48067a85f7020d84b04fbb48a2d38`，工作树含既有修改，以 [精确源码/运行身份](/tmp/opencut-package-history-20261022/source-identity.json)为准，不能用 HEAD 代替实际源码。

原 **48 工程 / 916 修订 / 868 事务 / 288 任务**，状态验收后 **48 / 920 / 872 / 288**。仅合成工程新增两次编辑、两次撤销；最终完整文档 SHA-256 与基线一致，为 `feac15c345134185ae6b5a8846606a5753aaed249c13bddd74c5dd7c49f43fb3`。原任务行哈希不变，零新增渲染或封面任务，SQLite `quick_check=ok`。测试账户已禁用且有效凭据为零。最后只读复查没有增加修订或事务。

[机器可读记录](package-location-history-20261022.json) · [原始证据目录](/tmp/opencut-package-history-20261022) · [真实操作结果](/tmp/opencut-package-history-20261022/browser.json) · [最终只读四组合复查](/tmp/opencut-package-history-20261022/browser-followup.json) · [服务切换](/tmp/opencut-package-history-20261022/deployment.json)。

[具名操作者与完整前后值](/tmp/opencut-package-history-20261022/browser/zh-light-history-values.png) · [中文浅色历史](/tmp/opencut-package-history-20261022/browser/final-zh-light-history.png) · [当前到目标的恢复对比](/tmp/opencut-package-history-20261022/browser/final-zh-light-restore.png) · [选择保存位置入口](/tmp/opencut-package-history-20261022/browser/final-zh-light-save-location.png) · [实际包保存成功](/tmp/opencut-package-history-20261022/browser/package-saved.png)。

[已校验的可编辑工程包](/tmp/opencut-package-history-20261022/selected-project.opencut) · [包文件校验和](/tmp/opencut-package-history-20261022/package-SHA256SUMS) · [匹配新二进制的 Agent 插件包说明](/tmp/opencut-package-history-20261022/plugins/README.md)。新插件包未安装到全局客户端，旧包作为历史产物保留。

## 尚未验收的范围

系统窗口脚本已通过 macOS `osacompile`，但系统窗口实际弹出后的人工点击选址未在本轮执行；浏览器验收替换的是该窗口的 path/cancel 返回边界。原生选址当前仅支持 macOS，其他宿主返回能力不可用。完整读屏、跨浏览器/触屏、罕见权限/断线/进程故障矩阵、精确合成工程封面与全工程交付继续开放。未重跑无关播放、渲染、ASR 或用户真实媒体任务，也不将本轮记录改标为它们的新验收。
