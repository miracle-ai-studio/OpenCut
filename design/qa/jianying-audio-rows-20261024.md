# 固定音效目录单行试听与真实源信息

2026-10-24，以原`jianying-zh/07-media-audio-graphics.png`及七图42项出口为准。本轮只交付固定音效目录行；音乐目录、图形完整编辑、所有草稿/锁轨矩阵及冷起播仍开放。sol6.1/high独立实际浏览器验收限定通过，不以本轮结果代替整个七图或渲染质量验收。

## 实现与边界

- 单行呈现一个试听/暂停按钮、中文名称、完整源波形概览、时长、添加及按需来源信息；试听状态来自真实媒体事件，进度来自当前HTMLAudio时钟。波形不是采样对齐的播放游标，不能用它精确seek，也不作为工程落点或音量包络。
- 复用原只读目录试听端口、固定源SHA/大小与授权租约；播放前仍验证返回字节及SHA，等待Native暂停确认，退出/切源/连接换代取消旧读取并释放Blob及播放owner。添加/拖放继续使用原规划和事务；只读目录元数据不等于已下载或具备导入权限。
- 每条固定CC0源离线生成真实128采样全声道peak，编译进原Rust应用目录，不增加运行时解码器、缓存队列或新依赖。公共DTO从Rust声明生成SDK/OpenAPI；producer脚本、固定源及分析身份绑定，旧分析不能跨来源复用。
- 子Agent静态检查发现并修复已获取资源标题悬空`entry`、全局capture抢占行/summary键盘、SVG起点被HTMLElement过滤后误拖三项。真实dispatcher整合用例及SVG-retarget用例覆盖接线；summary原生行为另待真实浏览器。

## 源分析和独立参考

证据位于`/private/tmp/opencut-jianying-audio-rows-20261024`。初次流式拼接51条与FFmpeg均不符合原门槛，失败`art-all.json/main-pcm-oracle.json`保留。Classic独立音频走Web Audio decodeAudioData，视频才使用另一条Mediabunny路径；未为目录更改全局source-art拼接算法。

改用原Classic独立音频解码路径和共享`peakBuckets`：`art-webaudio.json`记录Chromium153.0.8010.12、Node/模块/producer身份、完整源hash、两种时钟及真实peak。生产UI只读编译后的信息，不在每行启动解码。已有固定归档/素材复用，公共音频字节与解码证据保留仓库外。

独立PCM门槛保持样本数误差≤1、bucket数一致、最大peak绝对差≤0.015。43条与原始FFmpeg直接比较通过；另外8条原始比较失败仍保留。8条Ogg尾包具有无符号负duration及超过终包帧数的discard；参考由独立ffprobe包信息先冻结源速率裁样规则，再由FFmpeg解码，未按候选长度拟合。8条适配参考均通过，最大peak差0.000224173069。`main-tail-recipe-audit.json`另只读校验源SHA、负duration、discard边界及正extent，未重复解码。

官方[AudioDiscardHelper历史源码](https://chromium.googlesource.com/chromium/src/+/fbe5d0aef57d3481ff11a0b21f2dd87a30c40d3c/media/base/audio_discard_helper.cc)支持超量尾裁样丢弃整个buffer的机制；该历史commit不证明本机153二进制的精确源码。因此明确分列43直接参考与8容器语义适配参考，不写成51条原始FFmpeg全部通过。

`main-runtime-metadata-check.json`证明服务实际返回的全部51条source SHA/大小、producer/analysis身份、真实观察时长及f32 peak逐字节匹配编译分析；非音效条目的audio为空。这不是整个音视频渲染输出或跨解码器普遍等价证明。

## 候选与检查

`candidate.json`固定27源码、284份生产Web及Native。Native路径`/private/tmp/opencut-jianying-audio-rows-20261024/runtime/opencut`，SHA`bf8f16e3ab6111e7879610b059b636b37bd775435505ec44ac1f958327f8f596`；一次release构建12分40秒。同一明确二进制生成/检查契约及canonical通过，186项工具/12领域。原生目录测试7项、架构14项/22模块/0例外及合同边界通过。

Web四文件28项相关检查通过（其中14项试听用例最终单独复查，其余保留同候选通过结果）；最初一次异步读取测试等待超时和新测试误选播放SVG的失败日志保留，后者修正为波形区选择器，不修改产品绕过断言。集成后一次生产构建、实际发现许可空行后一次CSS修复构建（均含TypeScript检查）通过，292文件边界通过；构建大chunk提示保留。最初两处类型错误已修正，未因Web修正重编Native。

本地原数据库服务管理式重启一次，新PID95774/实例61b63401-6de4-4883-9002-61a9368ac2c8；5173开发UI与59790静态均新候选。旧二进制及Web目录保留。`server-deploy-tail.log`只增加336字节正常stop结果（error=null）及start监听行，服务ready。重启使旧内存Cookie失效与应用故障分开审计。

## 独立交互与数据日志

独立`sol-ui-review.md`限定通过；主代理另阅实际截图及`main-final-audit.json`逐项复核。实际1280×720/1440×900/1920×1080均无页面横纵溢出，试听/Add/Info各28×28且中心命中；初次闭合许可行62px来自全局details的14px margin，修复后44px。保留初次失败截图和最终三尺寸图，不重跑已过行为。

真实row Enter启动switch33，暂停0.129831/0.515057秒且进度25%，续播自然结束进度100%；切switch34只保留一条活跃HTML音源，Esc暂停并清src。summary Enter打开/Space关闭/Esc关闭均保留焦点且不触发全局播放或归零。Play/Info的SVG实鼠起拖被拒，主体实鼠起拖显示真实目标HUD，Esc清理且零Drop；已有真实Add/Drop证据保持原候选范围，没有重复写入。

纯声音Native会话复用一次：Native playing后，pause dispatch于04:42:07.649Z完成200，HTML于04:42:08.087Z才play；反向在HTML尚未ended时，04:44:51.299Z先pause，53.177Z Native playing且HTML保持暂停。只捕获控制请求及响应status/ID，没有成功响应body，不声称精确回执cursor；不覆盖完整视频冷起播或物理音质。

4908个完成响应：4901×200、5×204、2×PREVIEW_CACHE_MISSING只读旧缓存404；4903个请求ID，零console warning/error及pageerror、无HTTP401/403/500。18个ERR_ABORTED取消保留，不全部归因。尾窗04:45:37.881–45.951共325响应（324×200+1缓存404）、1资源读取取消、零pageerror。最后保留唯一QA1200×959原路由、素材tab、无选择、时间0、场景声音已停止、0HTMLplaying/dialog/drag；runner51044退出且监听/计时/路由清理。

Native暂停dispatch请求ID`b0b744ea-517a-4bad-9d06-a7a6c4746a46`；两次缓存404 ID`3def13eb-49e2-4b17-a9b4-423b6ad28577`、`5bdb3c50-ca47-4637-a43f-2a607933bf40`。浏览器导航retire请求没有完成响应捕获，因此主代理另只读`main-session-close.json`核旧session HTTP200/state=stopped、lease_remaining_ms=0、output=null、failure_code=null，QA recovery record=null。最初额外事件读取遇到终态410使验收器中断的日志保留；修正为必要的inspect/recovery读取，不重放播放或控制。

主代理SQL证明QA129及用户49的head、完整原文SHA与事务数均不变；Job仍621成功/67历史失败/33取消，最新失败事件仍20957。8条新增认证拒绝96351–96358集中在04:32:41–42重启窗口，早于04:34:47捕获；未精确关联这8条的浏览器请求来源，不写成逐条配对。一次真实重连后无新增拒绝。日志只增加正常管理式重启336字节，之后104511字节/SHA42b874dc…保持。服务最终ready且同PID/实例。

最终27源码、284Web与6SDK本地哈希精确；`main-served-assets.json`额外证明59790实际提供的全部284份产物HTTP哈希一致。首页走既有/editor路由；最初直接/index.html的验收器404按原allowlist纠正，保留失败日志，不改产品路由。evidence-sha256.json统一固定证据文件，未在仓库保存音频原字节。

本轮仅提升固定音效单行及原单目标淡入淡出的组合出口；音乐/收藏、简洁音量、多选/全草稿围栏、图形完整执行、源图预算及完整七图剩余项继续开放。
