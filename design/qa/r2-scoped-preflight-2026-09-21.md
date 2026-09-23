# R2 受限项目预检验收

2026-09-21。完成受限 Agent 的固定修订预检入口与真实预检→预览动线；R2 的素材导入、项目插件授权以及完整 R3–R7 仍未完成。Web/GPUI 实现继续暂缓。

## 代码与契约

纯 Application 增加 ProjectExecutionRead，SQLite 在同一只读事务中验证当前凭据及 run_jobs、读取该项目指定 revision；返回可信 SourceAccess。事务关闭后再进入 Native Services。Application 在原生调用前拒绝所需但未锁定的素材，并把受限原生诊断转换为保留 code/实体 ID、不含原生路径的解释。owner 现有详细诊断保留。见 [预检契约](../architecture/scoped-preflight-contract.md)。

工具仍为 36 个，Job 仍为 8 类、数据库仍为 v7。旧 22 工具和媒体发布两工具冻结契约通过。35 个工具完整定义与已验 dev-a4808c3781b2 一致；resource_read 的能力分支增加可选 access.scoped_preflight（default=false），HostCapabilities 的默认 access 也包含 false；新宿主为 true。生成 TS／Rust 公共契约与 OpenAPI 已同步，无 required 字段、已有输入或诊断 DTO 的破坏性变更。

## 真实验收

最终 debug SHA-256：`0ef756bc80ca84068e8042af838fcf2f8fca0914fef866f1c82814a446f96542`。

`scripts/verify-scoped-preflight.py --mcp-sdk-root <官方客户端>` 的 8 组通过；证据目录 `/private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-scoped-preflight-vq3scuat`，含 report/trace、真实 PNG、MCP 输出。

1. owner 绑定真实图片后删除原文件；受限 CLI/HTTP 与 owner 的预检一致且 ready=true，所有 SQLite 表前后相同。
2. read-only 和 read+edit 均拒绝；跨项目、他项目 revision、额外宿主路径字段分别明确失败；缺失 sequence 返回结构化问题。
3. 独立构造历史未锁定修订，返回 ASSET_LOCK_REQUIRED/asset_id，不创建源路径或修改历史；纯端口测试另外证明原生后端调用次数为零。
4. CAS 同大小破坏与暂时移除触发诊断，没有回退已删除的源文件；CLI/HTTP 输出保留代码／实体，不含真实宿主根目录。
5. 实际绑定内容 hash 正确但不能解码的文件，触发真实 FFprobe 错误，受限输出仍不泄漏路径；head 改变后旧固定修订仍能预检。
6. 官方 MCP SDK 分别通过 stdio/HTTP，以 runner 与 read-only 身份检查正常、legacy、无效媒体与跨项目场景。
7. 同一受限身份提交预检过的历史修订，真实 Worker 生成 PNG，内容绑定下载 SHA-256 匹配；FFmpeg 独立解码 RGB 检查 64×48 红色像素。
8. 移除 run_jobs 后 CLI/HTTP 新请求拒绝；恢复后可用；撤销 credential 后再次拒绝。停服后的 owner CLI 回读保持正确。

对应 revision `rev_420195da2eed79a406044c92644a565e215c31957bc75300203771b6cfd99f4a`，真实 job `job_8bfd3de6a8314924aa3300cbe6cc3dc8`。

相关 Rust 78 项通过，包含原有 SQLite 实际中断测试；新增纯端口测试验证未授权／legacy 不进入原生后端，以及错误消息/details 脱敏保留实体 ID。Clippy -D warnings、SDK TS 5 项、生成一致性和依赖／源码分层门禁通过。日志 `/tmp/opencut-scoped-preflight-tests.log`、`/tmp/opencut-scoped-preflight-clippy.log`。首次生成检查正确检出新增能力字段的漂移，生成更新后重跑通过。

## 交付

本机 macOS x86_64 开发包 `dist/local/opencut-0.1.0-dev-0046d31b1081-x86_64-apple-darwin.tar.gz` 已通过仓库外 26 组验收，29 条记录命令全部 exit 0。原已验包保持不变。

- Archive SHA-256：`534891343b01c6f2e8315b61a0fa45061f8af039b73e5f1fe2dc4e15b76107d8`。
- Release binary SHA-256：`e39312fab041cd4cd50a19a16b254d49d9926d74ead4e036d92c39dff3481648`。
- 统一报告：`/private/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-package-acceptance-ciu9wn3x/report.json`，同目录 commands.json 保存全部命令／输出，scoped-preflight/report.json 为解包程序的本轮 8 组真实动线证据。归档旁 `.acceptance.json` 保存摘要。
- npm SDK 在仓库外实际安装，Rust SDK 源码包在仓库外独立构建。owner 绑定真实图片后删除原文件，项目凭证通过两种 SDK 预检得到同一 ready=true 报告，并拒绝未授权项目；固定 revision `rev_0279b62ae06d0236e9d9e8787c8b94401ea6a435baa1ee47c1c8c6abb8661204`。
- 同包通过原有插件发布、媒体发布、权限管理、来源链、受限 CLI/HTTP/双 MCP、会话与事件回归；真实上传、预览、导出、修复、RGB/PCM 解码和停服重开通过。
- 历史 16 组／14 格通过，报告 `/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-history-npxdwfck/report.json`；数据库仍 v7。OpenAPI 38 路径的全部本地引用可解析。


## 未完成边界

预检不等于提交保证；后续任务仍独立检查权限／内容／能力。权限撤销不追溯取消已获得读取资格的普通查询。当前仅本机应用层隔离，未证明 OS 沙箱、真实浏览器呈现或所有媒体格式兼容。项目范围素材导入与插件授权继续推进，完整 R0–R7 目标保持。
