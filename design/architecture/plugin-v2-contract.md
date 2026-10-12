# 声明式插件 V2

状态：2026-09-24 本机真实 CLI 动线已验；[当前交付台账](headless-completion-ledger.md)。这是一组新增工具与包格式，旧 planner-v1 manifest、WIT 和旧 22 工具 schema 不变。

## 包与安装

Owner 用 `opencut_plugin_v2_install` 传入本机绝对目录。包包含 `manifest_v2.json` 和所声明的 `specs/*.json`，manifest 必须声明版本 2、插件身份、版本及每个预设 spec 的 SHA-256。V2 安装在工作区 `plugin-packages-v2/<id>/<version>/<package-sha256>`；同 id/version 的不同内容拒绝，重复安装返回同一身份。Host 限制文件类型、相对路径、未声明文件、包和单文件大小，检查 SHA-256，以版本文件锁和原子目录改名发布。中断后遗留的 staging 目录可在同版本锁下清理。安装预算计入工作区 `plugin_package`。

此格式只装载数据：`caption_style` 预设把固定样式展开为现有 `caption.update`；`edit_template` 仅能展开空轨道创建、素材片段插入和字幕插入。模板不能携带 shell、WASM、网络调用、`asset.bind` 或任意编辑操作。单包最多 32 个预设、单次最多 32 个操作，spec 最多 512 KiB，包最多 8 MiB。

## 项目授权与使用

安装不授予项目权限。Owner 用 `opencut_plugin_v2_grant` 为固定 project/id/version/package SHA-256 授予预设名；独立 schema 22 `project_preset_grants` 表持久化授权、版本 CAS、幂等回执、outbox 和审计。`opencut_plugin_v2_grants` 按项目 Read 权限列出含已停用的记录。旧授权写入的同键重放只返回历史回执，不改变当前撤权状态。

`opencut_plugin_v2_inspect` 要求项目 Read 和当前有效授权；`opencut_plugin_v2_preset_plan` 还要求 Edit、RunJobs。调用者传固定工程 revision、预设参数和绑定名到 asset ID 的映射。应用层在该不可变修订中核验素材已 pin 且版本身份有效，将预设展开为普通 `Operation`，交给核心计划器验证并返回 operations、EditPlan、spec/package SHA-256 和授权 revision。调用方检查计划后，以原 expected_revision 和新的幂等键调用现有 `opencut_edit_apply`。提交仍受工程 head CAS 和当时的编辑权限约束；规划本身不写工程。

预设中的静态轨道、片段和字幕 ID 按普通工程唯一性规则校验；一次性模板复用时需选择尚未占用的 ID 或由包作者提供不同预设。字体与系统工具不是包内锁定内容，不据此宣称跨机器像素级可重现。
