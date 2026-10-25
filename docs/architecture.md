# 架构

OpenCut 是模块化单体。外部 Agent 负责创作规划，编辑系统执行经过授权的读取、计划、事务和媒体任务。

| 层 | 实现 | 职责 |
|---|---|---|
| 领域与时间 | editor-core、time | 精确时间、编辑规则、工程不变量 |
| 合成 | composition | 固定快照到 Render IR，保留来源与时间语义 |
| 契约 | contracts、api-contracts、media-contracts、job-contracts、job-execution | 公共消息、错误、任务和端口数据 |
| 应用 | editor-application | 权限、用例、事务、幂等和资源政策 |
| 分派 | automation | 注册表驱动的类型化工具与领域动作 |
| 宿主 | host-runtime | 原生存储、媒体、插件与 Worker 装配 |
| 存储 | sqlite-store、asset-store、storage-budget | SQLite、CAS、准入、发布日志和回收围栏 |
| 媒体 | media、native-services、project-render | 探测、分析、IR 执行与受限子进程 |
| 调度与插件 | job-runtime、plugin-host | 领取、租约、取消、恢复、隔离和发布 |
| 客户端 | sdk-rust、sdk-ts、apps/web | 输入、呈现、传输与观察 |

领域、时间和合成层不读取 SQL、文件系统、网络或宿主环境。应用依赖契约与端口，宿主创建具体实现。内部调用不绕到 CLI；公开 DTO 不泄漏私有路径，客户端不复制领域规则。

工程修改基于固定修订，先计划、后原子提交。幂等键绑定原始请求，未知结果使用原键恢复。Worker 的 attempt、lease、取消和 publication fence 决定能否发布，临时文件不代表成功。

浏览器执行包消费 Rust 提供的固定事实与授权资源，不重新推导工程时间线。解码、DSP 和 GPU 保持准确的依赖身份。缓存、队列、PCM、RGBA 与子进程各自受准入约束，源文件大小与驻留预算分别处理。

现有工程的格式读取、SQL 升级和共享渲染器继续支持已保存的数据。第三方 WGSL、SoundTouch、字体和图形的许可证及来源哈希属于必要运行和分发数据。

依赖准入以[审核基线](../design/architecture/dependency-policy.json)为准。静态门禁不代替行为及独立像素/PCM 验收。

Web 准备适配器消费公开帧执行入口，显示工作器仅导入 GPU realm/presenter 三个函数。两处精确脚本门禁拒绝显示工作器取得帧执行或原生宿主；这条客户端装配边界不新增领域依赖或修改审核依赖基线。
