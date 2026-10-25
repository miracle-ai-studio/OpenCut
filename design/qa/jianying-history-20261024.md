# 历史面板密度与因果记录

当前状态：sol6.1/high真实UI限定验收通过，见[独立报告](/private/tmp/opencut-jianying-history-20261024/review/history-review.md)。此文只覆盖04图历史行和摘要修正，完整七图42条出口继续开放。证据目录 `/private/tmp/opencut-jianying-history-20261024`，修改前报告 `review/baseline-review.md`、只读源码审阅 `review/static-review.md`。

## 问题与改动

修改前真实行高在1440×900为97.59375px，较窄窗口126.59375px；四动作常显且25px命中高度。改为54px行、32px操作图标、单行标题与完整title、操作者和紧凑本地时间；完整时间保留到秒。版本动作由右侧28px按钮按需展开，标题和加载数量/更早记录footer固定，详情与动作共用正文滚动层。局部Esc关闭动作或详情，焦点分别回到原按钮，外部pointer收起动作，不请求项目快照。

原最新125事务为真实因果Undo，却因label=null显示“创建工程”。复用原共享历史读取端口，在SQLite投影层读取已有typed CausalJournal产生Undo/Redo标签；不重写历史journal或增加DTO。原普通编辑标签保留。Web只有明确无父修订才将空标签视为创建；展开详情时保留因果操作前缀并显示固定前后修订的对象差异。继续按需读取所选两个快照及操作者，未为每条记录预载整工程。

共享注册路径保持 `opencut_project_history → 应用queries::history → WorkspaceRead::read_revision_summary → SQLite投影`；原Read授权先于历史投影，固定修订、因果引用、原子提交及幂等均保持。没有新模块/依赖或公开DTO形状变化，SDK和其它入口消费同一标签。版本查看、恢复确认/过期head拒绝、因果撤销沿用原端口，不新增内部CLI调用。

## 检查

- Web历史/文案/差异共23项通过：按需/失败重试/迟到快照丢弃、版本动作Esc/外点及焦点、空label非根、因果前缀、时区/紧凑日期、恢复对比/确认/过期head。首次测试类型断言使用了不受支持的getByTitle选项，已修正为检查TIME元素，保留失败构建日志。
- Native原真实编辑/因果Undo/重开/Redo的4项通过，增加共享历史标签断言，保留原journal字节、幂等及因果测试。首次编译的JSON错误映射类型不匹配已修正，失败日志保留。
- 架构14项、架构政策及契约边界、284文件Web边界通过；Web最终生产构建通过，大块警告保留。只对边框CSS作后续Web重构建，Native没有重复构建。

## 真实UI与服务核对

最终候选 `candidate-deployed.json`：Native `b3f2a78629fb37fccbc53579f35eefc28ad506c2bebdbd90ed128490d172a7f8`，186操作/12领域生成及canonical检查通过；原环境和原DB更新服务，实例`bd7891f2-9308-4472-9003-cfe44342cda5`、ready=true。Web5173源码三尺寸验收；59790实际返回的HTML与直接JS/CSS与最终构建逐字/哈希相等，见`served-web-proof.json`。候选7文件源码无漂移，鉴权修复6文件与前轮一致。旧候选媒体质量/性能证据不自动升级。

1280×720、1440×900和1920×1080，整个li及顶距均54px，summary42px/More28px/按需动作28px；真实wheel正文0→400、标题与footer位置不动，单正文滚动、页面零溢出。真实Enter/Esc/Space/Tab和外点收起、焦点返回通过。最新125初次显示撤销修改，展开只读两个固定快照与操作者，保留因果前缀和对象差异，收起保持缓存。长摘要省略且title完整，UTC22:09:13显示本地06:09:13。124只读查看→125当前返回时8秒片段按版本存在/消失，静帧ready；恢复124仅对比并取消，未提交写入。

主代理`main_final.json`独立核对完整projects与部署前精确相等：QA125/head fa7cf8cc、全文SHA768641c5；用户46/head c951f0e6、全文SHA2c52e4f1。零新工程/媒体/Job，既有558成功/67失败/33取消及最新failed20957不变。原Chrome唯一QA保留，客户端退出，无dialog/route/gate/drag/HTML媒体；最终声音实际为已暂停，未Play且不冒称stopped。

重启时旧grant产生6条authentication_rejected（96338–96343，UTC23:08:51–53），原页中文失效提示、5秒零请求；显式重连及普通reload回原QA。之后拒绝序号没有增长。主代理按4236响应的4190个request_id逐一查询拒绝表，全部零匹配，见`final-request-audit.json`。全段零pageerror/console/401/403/500，尾5秒52响应全200；server.log仅新增336字节正常停止/启动记录，复验期间未增。4个PREVIEW_CACHE_MISSING404、2个旧资源release的PREVIEW_STOPPED409保留原body/ID；27条ERR_ABORTED未逐条确认原因，不能笼统宣称零请求失败。旧AUDIO_HOST_FAILED底层原因与冷起播性能继续开放。

## 未关闭出口

32px操作图标明确提示没有保存的缩略图，不能算真实版本缩略图。普通行初次只显示真实操作标签，具体对象摘要仍需展开才产生；此切片不关闭“所有行集中显示具体对象摘要”。键盘可视tooltip、任意长账号/自定义版本名、已保存版本独立页、草稿影响提示、完整恢复与撤销、统一导出入口及导出记录另有出口，不能凭此切片计通过。新Native身份不承接旧候选的像素/PCM或冷性能结论。
