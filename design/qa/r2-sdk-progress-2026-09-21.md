# R2 独立 SDK 与生成契约验收

范围：R2 的 TypeScript SDK、同源 Schema/OpenAPI、仓库外消费与真实 HTTP/CLI 一致性。R2 整体仍未完成；Web、GPUI 实现与真实 UI 验收按用户决定暂缓。

## 交付

- `packages/sdk-ts` 提供独立 ESM 包 `@opencut/sdk@0.1.0`，零运行时依赖。22 个工具输入／输出和八类 Job 结果从 Rust automation registry 生成，Web 保留兼容导入。
- `packages/contract-fixtures` 保存生成工具 Schema、OpenAPI 3.1、可供后续 Rust 客户端复用的编辑动线 fixture。
- SDK 明确错误 envelope、未知提交结果、稳定幂等重试、固定 expected_revision、Job 接受／完成／取消等待边界。Moon 增加独立包构建、测试和生成漂移检查；未宣称远程 CI 已执行。

## 本轮实际执行

`node scripts/verify-sdk.mjs` 最终 11 项检查通过，报告位于 `/var/folders/kz/h5_cbtss31x7v_c7008mkn4c0000gn/T/opencut-sdk-Jtp57V/report.json`；前两轮 `opencut-sdk-UdC06k`、`opencut-sdk-32hVOS` 也通过。最终入口先构建 CLI，避免用陈旧 binary 验收。每次运行使用独立临时目录；报告保存 package/binary SHA-256、Node/platform、revision、事务、Job ID 和不含 token 的请求 trace。

1. 真正 `npm pack` 并安装到仓库外项目，不通过相对源码导入绕过打包。
2. 外部 TypeScript 消费者可调用已注册工具；缺 project_id、未知工具和 Job kind/input 错配编译失败。
3. OpenAPI 路由与 binary 的工具集合一致；所有内部 `$ref` 重定位后可解析。此检查不是完整 OpenAPI meta-schema 验证。
4. 真实 token 认证、capabilities envelope 与错误身份保留。
5. 工程创建幂等；只读计划之后独立 CLI 回读工程未改变。
6. 实际 HTTP 代理等待后端提交再断开连接。SDK 不自动重试；原请求手动重试返回同一事务及 revision。
7. 过期 revision 和改内容复用幂等键分别拒绝；CLI 证明 head 未被额外改写。
8. 事件 cursor 与 CLI 一致，重复提交只有一次 commit。
9. 真实 WAV 经 FFmpeg Worker 探测；对实际代理响应延迟后，本地超时／AbortSignal 只停止等待，不发取消命令；最终 Job 成功且 cancel_requested=false，CLI 结果一致。
10. 不存在媒体使 Job 真实失败，SDK 保留底层错误；明确取消的 Job 与失败 Job 错误分类不同。

另执行 SDK 五项传输边界测试与 Web 全量 TypeScript 检查通过。首次公共动线在验收脚本使用 `streams[].kind` 时失败；实际契约为 `codec_type`，核实源码与 CLI 结果后修正脚本，并增加 `jobResult` 类型入口；未修改生产媒体输出来迎合断言。失败目录 `opencut-sdk-MWqaAv` 保留。另一轮外部编译试图禁止开放 DTO 的未知字段读取，触发 unused expect-error；保留 Schema 的可扩展语义并移除不符合契约的断言，目录 `opencut-sdk-CYEg4B` 保留。

## 尚未覆盖

- Rust SDK、公共 Job DTO 与 Worker prepared/持久记录解耦、Application/存储端口重构、完整历史协议版本矩阵仍待完成。
- 包未发布到公共 registry。报告所示运行时为 macOS x64、Node 24.19.0；Node 22 下限、其他架构／OS、真实浏览器仍待环境验收。
- OpenAPI 尚未覆盖上传／下载／SSE；SDK 目前通过类型化工具读取事件，不提供自动重连或通用运行时 DTO 校验。
- R3–R7 的原目标继续保留，不以本次 SDK 动线通过替代协作、连续预览、分析、完整插件／工程包及发布验收。
