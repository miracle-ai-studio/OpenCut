# 暂缓客户端的接入约定

2026-09-21：用户明确暂缓 Web 与 GPUI 桌面实现。该决定不缩减服务端、领域、媒体、Agent 或工程交付范围；移动端仍按原设计独立跟踪。

## 所有权与命令

- Application 持有工程、revision、事务、事件与 Job 权威。客户端只保存连接、选择、视口和已读状态投影；不得直接修改数据库或自行解释编辑时间规则。
- 可写上下文为 connection identity、project、branch、revision 和本地访问代次。切换连接／工程立即使旧请求失效；旧成功请求可以已在服务端提交，但其回读不得替换新上下文。
- 编辑携带 expected_revision 和调用方创建的稳定幂等键；网络失败不能偷偷获取最新 revision 后重试。计划只读，提交必须显式走同一事务入口。
- 快捷键、鼠标、脚本和 Agent 操作最终表达为明确对象 ID 与类型化 Operation，不能让公开 API 依赖当前 UI 选区。

## 读取与反馈

- UI、SDK、MCP、CLI 共用生成的 DTO／Schema。兼容变更新增可选字段；破坏性变更需要新协议版本和迁移说明。
- 事务提交事件支持持久化游标、去重、断线恢复和过期游标后的快照重建。不同流的游标不可混用。
- 错误保留 code、message、details；客户端依据语义处理冲突、修复、缺能力和未知结果，不把所有 HTTP 500 当成安全可重试。
- Job 创建成功仅代表接受。取消等待与取消 Job 为两个动作；关闭客户端不自动取消持久任务。进度的 stage/fraction 不冒充总体完成百分比。

## 预览与媒体

- 工程帧、区间、连续 Session 绑定固定 revision，并区分源素材和工程产物。请求时间与对齐后的实际时间都可观察。
- 连续播放遵守 continuous-preview-plan 中的 owner lease、generation、背压、TTL 与资源清理；客户端只报告实际媒体时钟观测，服务端计算接收时间与 freshness。
- 服务端 producer progress 不代表用户已看到／听到的位置；没有新鲜客户端观测时必须标记未知／陈旧。暂停、seek、切换 owner 的失效规则对所有客户端相同。
- 媒体下载使用受授权 artifact identity，不能把服务器本机路径当远程可用地址。当前 v1 遗留路径字段的替换需保留兼容适配。

## 暂缓验收清单

真实浏览器拖拽／播放／出声、GPUI 操作、多窗口交互、浏览器独立存储与解码、UI 性能和视觉质量暂缓，明确不声明通过。相应服务端契约仍以独立 CLI／SDK／MCP 客户端、真实媒体解码、并发与故障恢复验证。

客户端恢复开发时，必须补真实宿主验收；协议通过不能自动升级为 UI 通过。


## 提交来源查询补充

数据库 v4 新增同源 `opencut_audit_query`。客户端可以按 transaction/job/request 查询提交来源及 Worker attempt/产物关联，但不得把当前 local-owner 解释为某个人或模型身份，也不能把 URI、ID 或该来源记录当作权限凭证。重试保留第一次提交的 actor，响应 request_id 属于当前尝试。见 [来源契约](actor-audit-contract.md)。Web/GPUI 不必为此提前实现 UI，接入时沿用共享工具/资源契约。


## 项目素材导入接入补充

客户端读取 HostCapabilities.uploads 判断可用性。创建会话、确认偏移、封存／abort 和素材查询使用注册工具；视频字节走 advertised PUT 路径，MCP 不内嵌 base64，也不把远程客户端文件路径作为服务端路径。CLI 可通过 `upload put <upload_id> --offset <offset> --input <本地文件或 ->` 提供单块（最大 4 MiB）。TS 的 uploadChunk 与 Rust 的 upload_chunk 提供认证／每块摘要封装。

上传失败保留会话 ID，先 inspect 确认偏移。seal 返回唯一 job_id，通过公共 JobView 观察，media.import 的扩展结果版本为 1；成功后展示项目素材，再由用户／Agent 显式提交 asset.bind 或 asset.replace。允许改工程内 asset id／显示名称，内容身份和元数据须匹配当前项目的权威素材记录。导入不隐式插轨、不修改 head。

素材列表分页必须保留 next_cursor，后续导入不会进入已开始的分页范围；刷新应重新开始。abort 的清理待完成错误表示取消已经提交，应提示重试清理，不能显示“上传仍在接收”。sealed 状态转交 Job 取消，不把 abort 当回滚成功导入。网页与桌面仅约定这些状态和协议，界面实现／真实交互验收仍暂缓。


### 上传暂存释放与配额

HostCapabilities.uploads.staging 指定四个工具及逻辑预留限制。opencut_upload_query 提供分页发现入口，默认只列仍保留配额的会话；游标固定创建上界／过滤条件，状态取当前值。界面／Agent 应区分上传状态、任务终态和暂存保留状态；“导入成功”不等于已释放暂存。提供明确的释放操作，展示本项目预留量；receiving 走 abort，sealed 且 queued/running 不允许 release。release_eligible 查询只作提示，执行仍可能因状态／权限变化失败。

清理成功不删除已导入素材或改动工程 head。UPLOAD_RELEASE_PENDING / UPLOAD_CLEANUP_PENDING 保留状态提示，查询后允许重试；不能表示“没有任何更改”。配额按完整声明大小计算，不显示为机器实际磁盘用量，也不披露其他项目使用情况。失败／取消导入采用新幂等键重新上传，旧任务／会话保留历史。完整约定见 upload-staging-contract.md；客户端 UI 仍暂缓实现。


项目插件增量接入遵循 [项目插件契约](project-plugin-contract.md)：只读发现内联 schema，按准确工程 revision 规划，显式提交操作。分别保存授权 revision 与工程 revision；权限/包身份变化丢弃规划结果，禁止凭历史 grant 回执推断当前执行权。Web/桌面实现暂缓不改变这套服务端权威。


拒绝错误与诊断遵循 [拒绝审计契约](denial-audit-contract.md)：展示原拒绝码并保留服务端 request_id；Owner 查询有界历史。记录不可用以 warning 表达，不能据此自动重发写入。MCP 资源订阅首次撤权后停止观察，恢复权限后显式重新订阅或连接。

存储盘点接入：`opencut_storage_inspect` 仅 owner，可跨四入口和双 SDK 消费；字节字符串按大整数处理，必须呈现 complete/stop_reason/omissions，不能将部分扫描显示为完整占用或可回收空间。参见 [存储治理契约](storage-governance-contract.md)。Web/桌面继续按约定暂缓实现。

CAS 回收：HostCapabilities.cas_gc 发布上限与 plan/query/inspect/execute 工具。必须区分审阅计划、执行及逐项终态；removing 为结果未确认，通信失败时重新查询。query 空页带游标时继续翻页，不自动提交计划，不以逻辑字节表示已释放磁盘空间。具体协议见 [CAS 回收契约](cas-gc-contract.md)；Web/桌面实际实现继续暂缓。


持久存储准入接入：owner 可读取策略和完整活动预留、显式配置或恢复；受限用户只能看到自身操作的准确容量错误。UI 必须显示 coverage 和 enforces_all_workspace_writes，不把保守 headroom 呈现为保证物理空间。降低额度不会取消已获准的写入，终态任务失败需新请求；详见 storage-admission-contract.md。Web/桌面实现仍暂缓。
