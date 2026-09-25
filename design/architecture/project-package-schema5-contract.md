# 工程包 schema 5：插件包字节与显式恢复

Schema 5 承载 schema 3 工程文档。它保留 schema 4 的 FFmpeg／ffprobe 锁和完整项目授权快照，并为每个被快照引用的 planner-v1 与 preset-v2 包携带原始包目录。schema 2、3、4 仍可导入；schema 4 不要求包字节，schema 5 则要求授权快照与插件包清单逐项匹配。

## 包布局与身份

插件目录按稳定身份放置：

```text
plugins/v1/{id}/{version}/{package_sha256}/...
plugins/v2/{id}/{version}/{package_sha256}/...
```

Manifest 的 `plugin_bundles` 按 `kind:id@version#sha256` 规范排序。每个路径必须与 kind、id、version 和 digest 精确对应。V1 字节使用 PluginHost 的目录哈希规则复核，宿主维护的 `.opencut-grants.json` 和临时文件永不打包；V2 字节使用 preset 包的清单、spec 声明和路径哈希规则复核。导入拒绝缺包、身份错配、哈希错误、符号链接、非普通文件和清单外路径。插件目录合计上限为 256 MiB，授权／插件身份上限为 128。

Schema 5 的 `dependencies.plugins_not_bundled` 必须为空，因为项目授权引用的精确插件包已被携带。字体仍是外部依赖；self-contained 指媒体字节自包含，不代表字体、工具或目标主机完全相同。`fully_reproducible` 继续只按 manifest 中的自包含模式及未解决依赖计算。

V1 的 `proposed_global_permissions` 是源机全局插件权限快照，只作为显式安装工具调用的提案；V2 必须为空。它们不会在导入时应用，Owner 可在执行前删减或拒绝。工程包不把 `.opencut-grants.json` 复制到插件目录。

## 导入与恢复

导入只验证包并发布项目；不会安装插件、设置 V1 全局权限，也不会写入任何项目插件授权。回执的 `plugin_grant_restore_plan` 依次包含：

1. 对应每个包字节的 `opencut_plugin_install_pinned`／`opencut_plugin_v2_install_pinned` 工具调用。调用携带并在安装时强制校验 `expected_package_sha256`，避免导入检查后包目录被替换；V1 调用展示 `proposed_global_permissions` 快照，Owner 可在调用前删减权限。原有安装工具和输入 schema 不变。
2. 针对目标工程和 revision 0 的项目授权调用，使用来源快照的精确 package SHA、操作／preset 集合和启用状态，并带稳定幂等键。

Agent 或 Owner 必须先审查并显式执行全部安装调用，再审查并执行授权调用。每个调用保持自己的原子边界；整组恢复不是单个数据库事务。恢复输入引用导入目录中的 bundle 路径，因此执行这些调用前，该工程包目录必须继续可用。重放导入回执时不重新验证已删除的包目录，回执中的动作仅是历史计划，不代表当前插件安装或授权状态。

## 验收范围

真实 CLI 验收覆盖 V1 WASM 和 V2 声明式包字节搬运、manifest 与包哈希一致、导入零安装／零授权副作用、包在导入后被换成另一份结构有效内容时安装 SHA 固定拒绝、经 Owner 显式执行的 V1 权限提案与两类工程授权恢复、动作幂等重放、包内字节篡改拒绝、旧 schema 导入和后续编辑渲染。目标机字体仍需单独安装；整组授权恢复原子性、长期归档策略和跨平台发行验证不在 schema 5 内。
