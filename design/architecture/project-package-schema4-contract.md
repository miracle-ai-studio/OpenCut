# 工程包 schema 4：插件授权快照

Schema 4 是 schema 3 工程文档的工程包格式版本，不是新的工程文档版本。schema 2 仍表示旧的 FFmpeg／ffprobe 未锁定包；schema 3 包含精确工具版本和二进制 SHA-256；schema 4 保留 schema 3 的工具锁，并增加项目插件授权快照。导入器继续读取 schema 2 和 3。

`plugin_grants` 保存 planner-v1 的 `id/version/package_sha256/operations/enabled` 和 preset-v2 的 `id/version/package_sha256/preset_names/enabled`。项目身份和 CAS revision 不随授权配置迁移：授权作用于导入目标项目；目标是新项目，初始授权 revision 为 `0`。快照按插件类别、ID、版本排序，重复身份无效。`dependencies.plugins_not_bundled` 必须与快照中的插件包身份完全一致，包含准确 SHA-256；清单与快照不匹配时，整个包拒绝导入。

导入只发布经过验证的工程和媒体。回执的 `plugin_grant_restore_plan` 返回原授权字段供 Owner 审阅；不会自动安装插件、改变项目授权、加载插件代码或应用编辑。执行恢复前，Owner 必须在目标工作区安装与 SHA-256 相符的包，再用现有授权工具逐条写入，使用目标项目当前 grant revision 和新的幂等键。没有随包携带的插件二进制仍是外部依赖，因此含授权快照的包不会标记为完全可复现。

工程内容来自调用方指定的固定项目 revision。插件授权不是工程 revision 历史的一部分；导出在读取该 revision 的同一 SQLite 读快照中记录当前授权策略。清单不声称能还原该工程 revision 当时的历史授权状态。Schema 2/3 没有完整授权快照；如其 `plugins_not_bundled` 非空，导入继续返回显式的缺失依赖错误，不猜测或静默丢弃策略。

字体文件／版本不随本格式锁定。导入继续报告目标宿主缺失的字体族；准确字体内容、插件包字节、跨平台工具二进制和完整历史归档仍属于待完成边界。
