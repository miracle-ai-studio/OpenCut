# 声明式预设契约

2026-10-19：唯一业务契约的静态集成说明，尚未生成、构建或运行验收。当前状态以[交付台账](headless-completion-ledger.md)和[接线状态](static-migration-wiring.md)为准；原包安装、旧规划与输出证据保留各自来源，不提升为当前树通过。

## 包和权限

预设是数据，不执行 WASM、脚本、shell 或网络操作。安装、目录与包详情共用 `opencut_plugin_install/list/inspect`，具名预设使用 `opencut_plugin_preset_inspect`。既有包的实际清单文件、格式标识、精确版本和 SHA-256 是不可变依赖身份；它们不再决定公开业务操作的代际。

继续复用原目录/文件类型、依赖锁、摘要、未声明文件、共享预算与原子发布规则。单包最多32个预设，单次最多32个操作；原512KiB spec、8MiB包、64KiB参数和4MiB工程元数据预算不变。这些是数据/驻留预算，不是素材源文件长度上限。

安装不授予工程权限。`opencut_plugin_grant/grants` 管理或读取固定工程、包哈希及具名预设的授权；原授权CAS、幂等回执、审计和outbox保持。规划要求当前Read、Edit和RunJobs及启用的精确包授权。

## 唯一规划与提交

`opencut_plugin_preset_plan` 接收 `target`、`preset_name`、固定 `revision`、最终事务的 `idempotency_key/label`、`parameters` 和 `asset_bindings`。可选 `branch_name` 选择已存在的命名分支，省略表示main。

素材绑定仍只接受该固定修订中已有的不可变素材。新字幕通过 `caption_track_bindings` 指定cue ID到所属字幕轨道ID；轨道可由同一模板的前序操作创建。`font_bindings` 把预设声明的字体family显式绑定到该固定修订中已导入的字体ID，不查找宿主字体或尝试隐式回退。无用绑定、非字幕所属轨道、锁轨、重复插入身份、缺失资源及超出原数据操作白名单均拒绝。

返回 `package_sha256/spec_sha256/grant_revision`、一个普通 `transaction`、完整公开 `preview` 和可审阅的 `normalizations`。调用方检查后显式调用 `opencut_edit_apply` 提交该事务；规划本身不创建修订、Job、幂等回执或产物。事务固定head和当前权限在提交时再次检查；规划末尾重验授权和固定head，发生冲突后不得自动替换revision重发。

`caption_style` 只更新既有字幕的样式，保留原时间、文字、来源锚点和所属轨道。样式颜色、单位等复用原转换算法，显式字体绑定采用当前固定字体布局并记录规范化事实。`edit_template` 继续原空轨道创建、素材片段与字幕插入白名单和参数展开算法；媒体的原源时间、精确速率、参数和关键帧转换直接复用既有纯转换器。没有把整个工程投影或改写为旧模型。

所有结果交给同一个完整编辑规划器，因此现有文字/图形/蒙版/效果、轨道及曲线继续保留。新增身份使用已有聚合校验器收集的完整领域ID集合，包含曲线段身份，CAS内容版本ID不混入编辑身份命名空间。历史完整转换仍保留原生成身份；新预设身份不带开发代际编号。

输入不可表示返回 `UNSUPPORTED_PRESET_CONTENT`、缺少显式字体绑定返回 `FONT_BINDING_REQUIRED`，HTTP均为422/CLI退出2；历史工程需先转换时返回 `PROJECT_PROMOTION_REQUIRED`，HTTP422/CLI退出6。这些是可处理的前置/内容问题，不落为未知服务端错误；撤权、固定head冲突和锁轨沿用原分类。

## 验证边界

静态接线尚不证明编译或实际输出。数据包保留的历史媒体布局必须继续贯通来源、IR和后端；当前唯一导出及相关预检的完整整合仍在队列中。计划返回公开预览并不表示可渲染，也不表示独立像素正确。

`scripts/verify-preset-planning.py` 只复用显式二进制和一个已有专用工作区、工程、包、素材及字体，按完整规划/提交/重放/冲突和基线恢复执行；不复制夹具或重做安装。原红底/黄色字幕的三个独立像素判据保留为显式可选输出阶段，至多产生一个可恢复的输出Job。脚本目前只完成语法检查，没有运行；通用渲染、长源及最终交付仍需实际证据。
