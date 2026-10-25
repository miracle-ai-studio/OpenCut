# 工程插件规划契约

2026-10-19：唯一最终业务契约的静态集成，尚未统一生成、构建或运行验收。当前源码状态见[接线状态](static-migration-wiring.md)，已验证范围见[交付台账](headless-completion-ledger.md)；历史候选的结果不改标为当前树通过。

## 入口与权威

全局安装、依赖版本、WASM权限及发布审计仍由owner管理。安装不授予工程规划权。`opencut_project_plugin_grant/list/inspect/plan` 是唯一工程插件动线，CLI、HTTP `/api/tools/{name}`、MCP及生成SDK共用应用语义。原全局、基础和开发代际规划入口退出。

|入口|请求/结果|权限和副作用|
|---|---|---|
|grant|target、精确包哈希、贡献名、授权CAS、幂等键 → Grant|owner更新原授权/回执/审计/outbox，不移动工程head|
|list|project_id → Grant[]|工程Read，包含停用策略；不枚举全局安装|
|inspect|target、贡献名 → 参数/贡献schema、有效限额、Grant|工程Read、启用的精确包授权和全局WASM权限；无安装目录|
|plan|`PlanProjectPlugin` → `ProjectPluginPlan`|所有调用方均须Read/Edit/RunJobs和启用的精确包工程grant；返回提案，不提交|

规划输入为 `{plugin:{id,version,operation_name,project_id,revision,parameters},idempotency_key,label,branch_name?}`。包version是不可变依赖身份，不是公开编辑协议选择。返回 `{grant,transaction,preview}`，不含开发代际计数；CLI `plugin plan`、SDK `planProjectPlugin` 和MCP插件域的 `project.plugin.plan` 调用同一入口。

`branch_name` 省略为main，显式值选择已有命名分支。应用层在WASM工作前使用普通编辑的固定head读取/校验，再投影完整公开工程；不把现有文字、图形、字幕、蒙版、效果或曲线投影成旧模型。历史工程先显式转换，再规划编辑。只读提案可在owner编辑持有期间计算；真正提交仍受持有、权限和head围栏。

## 固定依赖与执行

实际清单选择一次私有函数绑定。原两种已发布WIT函数身份保持精确，必要的旧依赖元数据字段只在要求它的私有边界注入；两种绑定都接收相同完整公开数据，返回同一种普通公开编辑操作。没有调用方版本选择、失败探测回退或旧业务JSON翻译层。返回已撤销开发协议的组件明确失败；安装/恢复旧依赖的原身份不改写。

默认示例和固定ABI夹具共用一个提案实现。示例的 `pinned-editor-abi` feature只生成精确依赖函数夹具；不是另一套业务实现。静态集成没有重编现有组件或改写包字节。

原版本共享锁、包哈希/全局权限/schema校验、受限component读取及空WASI、内存/fuel/epoch/输出上限保持。输入JSON只在选定绑定后序列化一次，按实际传入字节准入。原有效上限为64MiB线性内存、1000ms实例化/执行epoch、500万fuel、256KiB输出；参数256KiB、快照4MiB、schema256KiB、component16MiB，进程内最多4个并行工程插件检查/规划。这些是执行/元数据预算，不是素材文件长度上限。锁等待和原生编译不计入WASM epoch，输出限额也不等同于宿主RSS上限。

## 固定修订、撤权与提交

原始工程grant和principal policy revision形成授权ticket。原生版本锁内、component编译前和执行前重验；应用层在返回后及完整编辑规划/资源处理后重验。原始ticket被撤销再恢复，也不能交付旧ticket结果。WASM不持有工程存储/写端口；数据库读取事务不跨执行保留。

规划只接受1–32个普通公开操作，交同一完整编辑规划器检查固定head、锁轨、资源和结构。`transaction` 使用原请求的key/label/branch；调用方审阅后显式 `opencut_edit_apply`。实际提交独立校验当前直接编辑权，保持原幂等、CAS、原子日志及发布；工程插件授权撤销不撤销原本具有的直接编辑权。head移动后返回冲突，不自动替换revision重发。

公开错误保留code并过滤宿主路径、WASM栈和原生详细值。撤权/精确包不匹配为NOT_AUTHORIZED，全局权限缺失为PERMISSION_DENIED，包损坏为PLUGIN_INTEGRITY，参数不合法为INVALID_ARGUMENT，输出不符合唯一编辑契约为INVALID_PLUGIN，执行预算耗尽为PLUGIN_RUNTIME；实际领域计划错误保持原分类。

## 验证范围

原真实组件/fuel/权限测试保留并改接完整公开输入；原基础模型的撤权单测由已有完整模型测试中的运行后撤权/拒绝交付覆盖，未另加重复单测。当前只检查源语法与注册，没有执行测试。

`scripts/verify-wasm-planning.py` 复用同一显式候选、已有专用工程、包、授权及同候选服务，执行CLI、SDK经HTTP和MCP提案一致、显式提交/重放/冲突与完整内容恢复。成功故障证明在粗阶段内保存，且绑定实际SDK代码和可选最终连接包身份。不安装组件、复制工程、创建媒体Job或制作中间包。第二固定ABI、历史业务负载拒绝/有界执行、运行时故障、撤权及最终连接配置只有在其对应选项实际运行后才成立。

包发布/预算脚本只核自己的发布、回滚、准入和权限元数据，不再重复WASM执行；最终归档验证引用已保留的同候选规划证据。完整效果执行、长源、平台矩阵和最终交付仍由当前计划及证据逐项证明。
