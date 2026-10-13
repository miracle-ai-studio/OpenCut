# 功能交付台账

更新：2026-10-13。唯一实施顺序、36项完整任务与验收条件见[迁移执行计划](../plans/classic-migration-plan.md)；被测候选、独立预期和范围见[验收摘要](../qa/acceptance.json)。本页维护当前状态，不把历史结果当作最新工作树全量通过。

## 当前范围与状态

目标是本机 Rust Server + Web 的 Classic 全量迁移。用户已排除 Classic 云账户/登录；字体/贴纸沿用 Classic 来源及允许的许可；音效允许开放许可来源或自行生成；反馈先用本机 mock。Desktop、移动端、跨平台签名发行及原版之外的专业能力仍是独立目标。

|Classic状态|数量|
|---|---:|
|指定动线已验|20|
|部分迁移|20|
|未迁移|14|
|实现待等价验收|2|
|原版占位|1|

57组中36组仍需实施或验收；S1–S6主责组为8/5/10/8/2/3。另有13页面与X01–X03覆盖核对，不重复加入功能总数。迁移未完成，不按组数计算百分比；全部F ID/状态及未完成集合保存在摘要和计划。

2026-10-13已重排执行计划：S1有界诊断之后进入固定字体文字，提前贯通共享V4视频输出；其余矩阵按实际依赖回收，不再以S1全矩阵阻塞独立功能开发。具体队列及退出条件见计划0.1–0.2。B0有界诊断现已交付，证据36条；57项状态、36项未完成及原开放残差保持。用户已授权三个独立模块并行开发；B1字体解析、文字栅格与Web表单正在集成，尚无产品入口/真实字体PNG验收。

## 已实现及有效证据边界

|功能范围|已实现与独立验证|尚未证明|
|---|---|---|
|旧产品与恢复|工程库/场景/素材、上传/断点续传、字幕交换、基本动画/视口、独立Server HTTP/MCP/Worker；F06旧库只读发现、损失确认、跨源桥及恢复后编辑有历史产品验收|生产旧源CSP、全部浏览器/非空历史矩阵；旧包/PID不证明当前就绪|
|F57反馈|编辑器入口、5000字校验、草稿/回执/最近20条、失败重试/删除、真实浏览器无反馈网络请求|范围采用mock，不发送真实邮箱|
|V4权威与历史|完整typed快照、固定授权资源、原样原子事务/幂等、重开、选择性Undo/Redo；DB26→27、27→28及28→29保持原历史/回执与V1恢复原json/digest，碰撞回滚；稳定ID与单位/通道绑定/精确时间/速率保持原子语义|完整Classic转换、branch/merge、历史包6与用户库升级；旧不兼容接口明确拒绝V4|
|纯IR及Legacy帧|全局/显式Legacy设置、原始时间值Bezier/旧native曲线profile、过冲物理参数、音频参与排除；六项实际Legacy像素覆盖媒体/形状/效果/蒙版/交叉淡化|纯模型/IR不证明所有后端能力；源request span不是解码窗口，实际PTS保留|
|F08全局FPS与VFR导入|锁定Mediabunny直接computePacketStats(100)，保留旧header；raw先比较后标准化、unknown抑制header回退及实际放置告警；fixed-revision/版本授权观察、旧V3兼容。真实180帧77KB VFR：header675/26、前100包约60→全局60；两非空场景完整内容/源1/7/关键帧/书签不变，一次原事务、apply/refresh/Undo播放头1001/2000保持，Undo及reload恢复内容/settings，0pageerror|F08部分迁移；真实unknown视频、当前V4 VFR视频导出及更广格式/长素材未验；reload播放头按产品归零；Node heap/cache非hard RSS配额|
|F10书签及Classic静帧|同V4工作区时间线标记/inclusive备注/选择性历史；1/3与2/3时间保持；Video/静态PNG/JPEG normal持久PNG、contain/翻转/旋转/层序/透明度/纯色背景有独立像素，HUD排除PNG|F10部分迁移；连续normal声画小样本见下项，正常分辨率/密集/长源同步、完整视觉类型和备注排除视频导出|
|S1连续normal声画|Rust固定修订/原凭据/lease/generation/overlay、source→IR→WebGPU、同帧准备与Worker两槽沿用。消费优先与紧迫租约保护后，新补document pagehide经原SDK keepalive Stop，立即静音/冻结/取消，不虚构final cursor；迟到新generation按新回执清理，未知create/Play的BFCache原key保留。最终21生命周期及未影响7桥接/Worker用例、SDK传输23/边界9、Web边界184/SDK+Web187、架构14/22模块/0例外和契约边界过；SDK构建1，Web集成及BFCache覆盖缺口修正构建2，0 Rust/native构建。最终源实际playing/paused整页导航：原session stopped、捕获子进程和Worker退出，原生树只剩Rust37–39MB。前置退休候选同一现场串行HD4与300秒14门槛均过：HD冷启4770ms、4秒96不同请求时间帧（非独立PTS）、偏差85.33ms、seek603ms、440Hz/RMS0.088316、一次资源下载及暂停HTTP权限拒绝清屏；持续终态复制14400000，20区间/19间隙384独立颜色、PTS152–172/同步122.67ms、分列资源及实际退出过|300秒证据明确属于BFCache未知key末修正前c6f8087 Web身份；最终90f996c源只补受影响的原key恢复单测/构建/真实文档离开，不重复300秒，不冒充全树复测。31窗最大量子缺帧0.8%、停顿0.7911%、confirmed deficit0.7861%，整体5632/14405632≈0.03910%；原生树2061082624B<2GiB、21进程，page heap增32.60MB、Worker22.74/16.87MB、GPU18.43MB分别过。一轮冻结夹具通过不证明旧长尾全已消失或因果性能收益；原产品失败和Native诊断保持身份。初始4096左声道PCM保持原范围；本轮20声源/19间隙双声道interior补充见下行。完整gain-seek-scene-revision-原凭据/真实BFCache未知结果恢复、无pagehide的崩溃/网络与租约后备、真实长源/VFR/非单位速率/跨浏览器/DAC仍缺，S1/F10/F42/F44和36组不标绿|
|媒体工作区与时间线|真实media ID/version贯穿上传、恢复、源试看、绑定/替换/放置；Video/Audio/Image多选Move、双边Trim/Split/Delete逐轨ripple、状态/空轨、原样Review/重开/UndoRedo；1.5x非零源跨度有独立预期|动画/非媒体完整矩阵、全元素命中；只新绑定比较导入FPS，不把重复插入当重新导入|
|手势、剪贴板和恢复|实际跨轨Move/Trim、S/Q/W、缩放滚动命中、框选Escape；同工程历史跨场景Copy/Paste/Duplicate及原生C/V；历史timeline-only整页reload原receipt/原key证据保留，迟到API/read不能降级Saved或复活pointer；F16指定轨道真实文件批次与Chromium trusted PNG粘贴已验|完整选择URL、Finder跨应用拖放、原生视频/音频文件粘贴、系统IME及跨工程未交付；通用V2当前范围单列，不以旧timeline证据代替|
|F16文件放置|复用上传/原生只读workspace规划/原样V4事务与恢复。拖放捕获缩放滚动后的显式轨道/时间，粘贴捕获所选轨道/播放头；图片默认5秒，批次按观测时长生成有序放置意图，实际对齐/类型/权限由应用规划决定。整个批次完成前不保留可重试的部分绑定；取消或失败的已上传素材仍可恢复。真实视频/PNG一次提交、重开与Undo/Redo；双WAV zoom2+scroll120落点1.25/5.25秒；trusted原生PNG粘贴12.25秒、输入焦点、混合批次拒绝、真实chunk取消、锁轨先拒绝均通过。精确确认补充批：逐片段展示目标轨道、请求/实际落点与源/时间线时长，解释帧对齐和主轨起点调整；失败保留原始请求，不伪造实际结果。真实48480帧/48000Hz WAV保持1.01秒，1.26秒请求在24fps落为1.25秒，原事务/UndoRedo/重开一致；批内与已有片段碰撞原子拒绝，第二文件准入失败后首个完成媒体免重传恢复，临时规划失败原ID/key/意图重试、主轨2.01→0均通过|F16保持部分迁移；Finder实际拖放来源、跨应用文件与原生视频/音频粘贴、系统IME/跨浏览器及更广格式/源时长/失败组合仍待验；显式轨道不自动新建/改靶。HTTP错误回复为故障注入，不推为所有原生存储故障通过；本批无新像素/PCM/连续声画证据|
|通用未知写恢复V2|typed PublicApply/Undo/Redo/Restore，新stage/inspect/ack三操作；DB29兼容原V1，混合单条2MiB/32条/64MiB及原凭据scope互斥；当前原动作权限/write-control、只读原family/key/full-intent与完整事务/因果回执、Immediate精确重复与原子ack；Web共同协调器仅存sessionStorage非敏感指针，整页原意图/原key恢复不replan；最终四family×两路径8案例及aggregate全部通过，0 pageexception；Restore实际替换默认场景、删除main后恢复历史main与独立书签，原意图/key、一次提交、无replan均断言|限定四类fixed-head写已交付；创建/复制、Job/upload、跨工程不混入此恢复证明。旧Web尾项分段证据不能当最终网页全套通过|
|F42完整/范围WAV|分离保留trim/rate/gain/通道并关闭视频音频，恢复保留独立音频；持久任务原caller权限/CAS/预算/取消/租约发布与下载贯通；真实1.5x/−6dB输出64000帧/512058字节、独立660Hz；范围6000/24000输出192058字节|F42部分迁移；范围为整场母带后裁切，不是实时seek。全量codec/edit/音高矩阵和视频导出仍待|
|独立PCM与decoder|Float脉冲trim/placement/EOF及0/−60/+20dB、mute；−60是非零；PCM PTS间隙在export拼接；first mono/default stereo实际primary；aligned AAC25包/ELST1024/末包448→24576帧，峰4096/16384及trim后3089/15377|AAC仅单个aligned LC夹具；非整包edit、任意codec、显式stream/复杂ISO明确未验或拒绝，不静默当primary|
|Classic DSP及实时声音|锁定SoundTouch0.3.0：441Hz两倍线性→882Hz/保音高→441Hz；真实WebAudio整场peak2后母带再裁切；ordinary保留PTS/EOF gap，prepared拼接/60Hz amplitude ramp；实际PCM659.95/440.07Hz、固定seek/目标临时gain、Worklet消费4096、暂停8896/48000、同session新gen继续|独立pre-master/whole-master/range证据分开；完整realtime PTS/AAC/post-master声学和物理设备未验；普通>2声道/显式stream/不支持edit拒绝|
|F44音量线|−60…20dB/0.1dB、14px命中、草稿/Escape不写、松手一次原事务、键盘/独立mute；动画不被静态覆盖、Legacy linear不偷换；实际两声部只变目标gain，误差6.47e−9；新head停止旧session|F44部分迁移；缩放/所有元素完整命中矩阵，Track无音量线；Video入口为明确扩展|
|真实权限/取消退出|四项分阶段真实生命周期/六源realm覆盖凭据idle/8192背压、Read-only撤RunJobs/空grants撤工程、自然30s lease及Stop；Node/Chrome身份树退出、CAS独占gate恢复；启动期延迟started取消实际无孤儿|完整HTTP断开/多宿主故障与全部codec矩阵；macOS为monitored-terminate，非hard内存/VRAM配额|
|密集工程|100片段/5轨/300秒同4秒CAS，private PackedV2保事实、单次decode、20独立Float seek；最终实际AAC打开846ms、冷声音3181ms、seek145–211ms、输入p95 32.6/max33.3ms、heap增量30021656B/retained增长231132B；10s消费477824帧、stall0%、deficit0.149%通过冻结1%门槛，场景URL刷新成立|仅指定合成fixture/宿主；不是长文件/VFR/物理声音或无限时长漏内存证明。4条seek中断控制台信息未精确关联，不能称0网络噪声|
|F17源层/Classic显示|历史独立JPEG/四声道PCM/PTS/无帧/300秒源证据单列；当前host纯10+实际1证明100请求复用、pinned容量/淘汰/停机/原凭据。最终CLI/Web10组通过固定head刷新、rapid部分裁切、PNG/JPEG/短CFR末帧/12秒PCM；隔离生产浏览器cache实际峰32830700B/256项，302个URL全部释放、最终0B；主UI性能另计|F17部分迁移；其他支持图片/源格式及完整长素材组合未验，隔离cache不冒充主UI/native实际占用；旧首次thumbnail不可用原因未确认|

## 候选与检查如何引用

摘要以36条唯一范围证据保留各自CLI、Web入口、native测试/执行身份及独立预期；旧候选明确historical。不同候选、staged库检查、浏览器前缀/专项不能合为同一最终候选全过。单后端输出相等、注册工具或构建成功不能替代独立像素/PCM及权限生命周期。

|最近已验范围|结果与边界|
|---|---|
|历史手势/剪贴板/timeline恢复|Rust373/Web82/SDK30分阶段；真实22组为前缀14、恢复/手势5、剪贴板3，各自CLI身份保留；不是最终CLI连续22组全跑。历史187工具，旧181及inspect前186不变，不作为本批工具数|
|密集声音|受影响host7并发/native20/Web19、真实四生命周期及最终release性能通过；上述Float原生与AAC浏览器是不同候选/夹具|
|F17缓存维护最终候选|host纯10及单项真实Chromium1、Web19、类型/生产构建/Web边界181、架构14/22模块/0例外/契约边界、22旧wire通过；190工具无声明变更；同最终release/Web实际10组通过。早期native/应用/Clippy/SDK结果单列为历史证据|
|F17实际浏览器独立范围|原WAV字节峰/高度/burn/Canvas像素及−12dB原事务回归；密集首中末/20zoom+4次rapid半clip，DOM打开837.3ms、首次video/audio艺术3156.5/3286.7ms、输入p95 26.1/max26.9ms、heap增量15716540B；0pageerror/0未分类失败。38主动waiter abort和6笔同request fulfilled204单列。isolated cache峰32830700B、256项、淘汰重建后0B，不将heap当cache占用|
|通用恢复V2本批分阶段|App5/SQLite197/API1、Rust SDK3/TS SDK38/Web96（含新增Restore定位回归）；架构14/22模块/0例外、Web边界15/176、SDK边界179、旧wire22项、193工具且旧190声明不变、生成检查通过。最终CLI/Web8恢复案例及aggregate全部通过，0 pageexception；此前同CLI旧Web分段10前缀+9尾项通过，含双声部gain手势、media恢复/替换、timeline两组、gesture/rubberband及clipboard三组，0 pageerror/failed request，不能合为最终网页一次全套。已增加CDP完成状态与应用实际消费text观测，本轮body-unavailable completed为0；旧失败原完成状态未采集，不能据后续通过宣称根因已修|
|F08当前候选|App25/SQLite6/DTO2/host4、打包5/doctor6、架构14/22模块/0例外及契约边界通过；旧wire22、193工具及生成产物保持。一CLI构建复用Web，真实V4场景通过；最小runtime6文件/1066929字节复制验证并已清理，不冒充完整包安装通过|
|F16文件放置基线候选|受影响Web5文件33测试、类型/生产构建与Web边界176通过；同一新Web构建+原F08 release CLI真实6组通过，0pageerror。Rust/公共协议未改，核对二进制身份后复用，不重编/重复生成契约或重跑无影响媒体验收；不作为当前整仓/整包通过|
|F16精确确认与失败恢复当前候选|受影响Web2文件15测试、含类型检查的一次生产构建/Web边界176通过；同一新Web+原F08 release CLI补充6组真实浏览器边界通过，0pageerror。Rust/公开协议未改；复用原二进制，旧33测试/6组浏览器证据保留在原候选，不重复当成本批结果；无全仓/整包通过声明|
|S1连续声画前置切片|Rust应用精确时间1+声音会话6+每层零预算回归1、真实host原凭据/固定修订/generation1；最新受影响Web2文件18测试、私有音频桥17、Classic visual既有5、类型/生产构建及Web边界178；架构14/22模块/0例外、契约/生成与22旧wire通过，194工具且前193声明完全不变。Rust集成release与最终零预算语义修正各构建一次；JS/验证器修正不重编Rust，零预算补充不重复媒体运行；已过主声画验收只在受影响暂停/续播边界补跑，身份与范围见摘要|
|S1 720p执行与资源前置候选|Classic visual9项（含135/134字节预算边界和同资源多源时间/准入保护）、受影响Web7文件34测试分范围通过；既有测试二进制实际GPU层序/透明度像素1及video实际PTS/绿帧1通过，无Rust重编。类型/生产构建、Web边界178、架构14/22模块/0例外及契约边界通过。HD四检查通过；密集专项三个性能门槛失败，其余独立预期通过。最终多源时间保护仅补针对性单测/生产构建，既有HD/密集都是同时间共享源，不重复验收；原Rust/生成/194工具与前193旧wire范围复用前置候选。无全仓/整包通过声明|
|S1同帧准备复用当前候选|应用帧准备7（固定时间/政策/解码事实及原零预算）、最终host credit/stream及已过期准备缓存释放8项通过；最终真实原生1验证缓存绑定/未来时间/旧generation/原凭据撤权与实际进程/CAS退出。架构14/22模块/0例外、契约边界、生成检查与22旧wire/194工具、Web边界178及类型/生产构建通过；集成release与终态元数据释放修正各构建一次，后者仅补受影响缓存/原凭据与契约检查，不重复媒体验收。HD四检查通过，密集三个门槛仍红；其他Web/渲染器/F08/F16/恢复/DSP证据复用原候选。本轮新增有界控制RTT和PCM到达观察：report p95 13.5ms、PCM块间隔p95 129.2ms；不能据此直接归因于native或物理设备。减少Worklet观察实验已撤回，最终保留原逐量子回执|
|S1 Worker PCM数据泵当前候选|受影响Web9文件35测试（原生命周期23及Worker桥/端口/控制串行/共享cookie-bearer12）、独立量子观察4、一次类型/生产构建、Web边界184、架构14/22模块/0例外与契约边界通过；0 Rust构建，原ff8019c release/4037ed音频模块、194工具及22旧wire证据按原身份复用。实际HD4检查+密集13门槛同Web候选通过，沿用固定工程/修订/媒体，未重复导入/复制或回跑无影响DSP/导出。停止先断开并冻结、取得真实final cursor、等待在途消费并串行reportStopped，原控制代际围栏后才取消旧流；取消/超时/迟到回执不伪造cursor。无全仓/整包通过声明|
|S1持续观察当前候选|新增纯观察4及既有量子算术4、专项注册/runner语法/摘要一致性通过；产品源码、原Rust/Web/音频模块字节不变，0产品构建，复用原架构/契约/194工具与22旧wire及HD/10秒身份，不重复验。一次实际300秒到终态并退出，14门槛中11过、逐窗停顿/deficit/真实量子缺帧三项失败；必要失败回执保留，未反复跑同候选追绿。8192原始回执上限内逐窗carry核连续性，最大3751原始点；实际Worker onmessage进入前采样与CDP Worker heap、pid/start原生树RSS分列。验收器补供给溢出保护及明确需既有密集元数据，不重编或重复现场；未扩产品队列/协议或弱化门槛|
|S1原生调度/GC历史诊断|旁路纯观察5及孤立Node GC API冒烟、语法/注册通过；0产品构建，原Rust/Web/音频包及同一工作区/工程/修订/媒体复用。新增GC观察实际300秒/3516命令/14400000供给帧、数值文件1709303B≤2MiB，无溢出/不完整命令；421个已交付GC事件，总889.70ms、最长16.14ms，保留最长32条；不主动GC/扩heap。当前最慢命令89.65ms，旧153ms返回长尾未出现；校准误差宽1.241ms。观察运行14门槛均过、全程真实缺帧8960/14408960≈0.0622%、最高窗口0.7467%，终态/资源/退出通过|产品未改，诊断运行波动不算性能优化，原逐窗失败仍保留；GC不解释/排除旧长尾。preview直连产品切片已交付，当前范围及失败见下行；本行观察仍属旧产品身份。旧153ms返回与Node116.64ms迟到重合只定位阶段，完整旧指标在历史摘要，原始失败文件覆盖及派生baseline按摘要说明；诊断已独立目录。S1及36组未完成|
|S1原生preview直连当前候选|既有host/preview23测试、新拥有者7测试通过；真实Chrome独立字节/receipt返回及关闭、强制group kill、启动取消均actual exit确认。架构14/22模块/0例外、契约边界及语法过，无公开schema/依赖变化。0 Rust/Web构建；显式ff8019c release/原Worker Web候选、同一合成工作区串行HD+持续+两项边界，HD共4过，300秒11/14过；必要32937B失败回执独立目录保存，同候选不重复追绿。默认其他backend路径保持，共享host源码导致模块身份更新；原194工具/22旧wire/原凭据Rust语义与未受影响DSP按原证据范围复用|逐窗三门槛仍红，下一步复用已有有界阶段观察定位新直连路径的late block/消费窗口，再针对已证实阶段改造；主服务停止、现场open handles0、preview Node actor0。无整仓/整包或性能优化声明|
|S1补块/控制响应体关联当前观察|有界观察13测试及语法/runner编译/原入口注册过；原Response/text保持，Worker→page不同timeOrigin回归已覆盖。0产品构建，同一显式Rust/Web/直连模块及合成现场，3516连续命令/14400000供给帧与32最慢补块的ID/start/count核对，原生数值1711658B≤2MiB、最终必要失败60782B单独目录。原先仅全程统计的等待现在可区分：command1952原生15.88ms，receipt→report171.8ms，其中前序report body154.4ms+heartbeat17.1ms，refill225.8ms；command3132原生165.12ms，evaluate返回122.336–123.212ms与Node迟到114.70ms重叠，receipt→report0.1ms、refill200.4ms且无前序控制重叠|诊断14门槛11过，第17/27窗真实缺6656/5632，各1.3867%/1.1733%；当前消费14416896/复制14400000/缺16896，整体0.1172%不遮盖局部。窗口是sample范围关联，非缺帧量子精确因果；后续SDK校验未单独计时，selected GC无命中不证明全无GC。PTS157–172/388独立像素/同步122.67ms及原分列预算、终态/设备/Worker/子进程退出过；现场占用0。该诊断已用于下项消费优先修正；Native返回长尾仍独立收口，当前产品局部缺帧及原生树RSS限制以下项为准。不重跑相同产品追绿或猜测混合器/扩大预算，S1和36组未完成|
|S1消费报告优先前一候选|旧pump针对三个新增回归先失败；修正后5文件33项（pump10）通过，连续40次待报告仍在租约前续期且控制并发峰1，迟到报告/heartbeat、真实停止游标及取消围栏覆盖。原heartbeat集成改用单调时钟；初次构建在测试mock类型检查停止，修正后仅一次成功Vite构建，无Rust构建/协议/依赖变更，原194工具/22旧wire及未影响DSP/导出证据按身份复用。原工作区/媒体/修订串行HD4和实际300秒一次；29条唯一证据，必要57472B失败回执独立保存|局部持续性和原生树RSS仍红，不重复相同候选追绿。当前最长消费报告等待及Native返回历史长尾需要按实际控制/调度阶段收口；RSS先检查原服务所拥有actor的退出/叠加及必要峰值进程归属，完整树预算不扩大、不排除旧子进程。现场open handles0、preview Node actor0，无整仓/整包通过声明|
|S1文档离开当前切片|旧实际整页导航3秒后仍paused、Node/Chromium原树996442112B，证明缺少文档清理；最终源playing/paused原Stop身份、真实子进程PID/start与Worker退出过，未知create/Play原key及迟到新generation由独立生命周期测试覆盖。keepalive沿用cookie/bearer SDK响应与错误语义，新增九项边界测试仅对齐既有Web获准公开视觉导出桥，host/相邻实现/其他文件仍拒绝，无依赖基线再生成。30条唯一证据，旧必要4133B退出失败回执单独保存|最终源未知BFCache分支只做受影响验证；前一集成Web的HD4+300秒14/14保持明确执行身份。未知结果与未派发pagehide/keepalive不可达仍靠原租约，未验证真实BFCache还原/崩溃/全控制矩阵；不声称RSS或Native返回长尾单一原因、永久优化或整仓/整包通过。现场句柄0、preview Node actor0|
|S1后续双声道PCM当前观察|最终有界观察14项（新增PCM7、已影响Worker回执/响应体/时钟7）及语法/注册通过；0 Rust/Web/SDK构建。复用90f996c Web、原ff8019c Rust/native与原100片段5轨/300秒固定修订及媒体；真实20声源+19间隙各4096帧/双声道共159744帧，独立440Hz/三声部−18dB RMS和间隙精确零全过。原zero流起点已观察、39完整声学窗口保留、0pageerror；最终源原8秒短场景真实终态384000/Stop384000、设备closed及head未变通过；部分窗口4096帧/完成队列8项定期清空、声明39项/无溢出，原转移字节保持|初次验收器把终态控制起点当成原流起点而失败，39窗口已经全部通过并保留，但300秒终态全汇总未到后续head断言；最终first-origin修正+纯回归后只补原8秒终态专项，保留有界失败测量避免再次丢弃已验声学区间；无产品重建或HD/既有性能矩阵重复。本项是相同mono源的pre-master双声道interior，不证明任意声部/立体声映射、精确边界/相位或post-master/物理DAC；原c6f8087性能14门槛及旧失败各保留身份。完整操作/BFCache/崩溃/网络/租约/长源/VFR/非单位速率/跨浏览器仍缺，S1和36组不标绿|
|S1快速seek已验范围|已修原第二个seek在final消费报告/accepted回执等待中invalidate误停播；一个最新精确意图/串行控制、接受代际后才续发，过期画面准备取消且无PCM流；Pause保持到最新目标，文档离开后迟到已接受代际按原owner/lease另Stop。旧四回归失败，最终26声音生命周期+未影响6显示/绑定用例、15有界观察/响应体门、Web边界184与一次类型/生产构建过，0Rust/native/SDK构建。同一原8秒夹具实际3边界过：latest seek263/362ms、同session/owner/lease、440Hz双声道pre-master/RMS0.088316、独立颜色与偏差117.33/37.33ms；两个seek加Pause原生代际增3，停在156000/48000、新消费0、设备closed/Worker0、head未变|最新Web40f17df只证明上述控制范围。Worker门与主线程路由分别拥有消息；首次验收器注入重复延迟/遗漏Worker、Pause代际预期均按真实语义修正，只补受影响专项，产品未重编，已过两播放案例不再重跑。旧300秒声学/性能及文档证据保留原候选；相同mono素材不证明独立声部/任意声道/post-master/DAC。picture取消/迟到文档代际是单测范围；临时gain交叠见下一切片；scene/revision/原凭据、unknown/BFCache/崩溃/网络/TTL和长/VFR/非单位速率仍缺，S1及36组不标绿。现场占用0、preview actor0|
|S1临时gain交叠已验范围|复用快速seek队列统一最新gain/seek及保持Pause；真实final消费一次、每个接受代际后续发，Escape取消过期画面准备且无中间PCM。暂停gain沿用已确认cursor，不报告退役pump或开声音；文档离开迟到gain补原身份Stop。六旧回归先失败，新增画面取消与Pause错误传播共8回归；最终34声音+15gain手势/事务+5画面共54独立用例、Web边界184过。首次集成后仅发现Pause拒绝传播缺口，定向修复/34用例及最终类型构建；共两次必要Web构建，0Rust/native/SDK。33唯一证据、57状态/36未完成不变。同一8秒场景三真实案例过：seek等待gain−12dB、accepted gain后Escape/seek/Pause、paused gain取消；独立PCM RMS0.022184/恢复0.088414及440Hz、5独立像素/偏差48ms，原head/owner/lease不变、设备closed/Worker0|实际成功路径绑定集成Web22f720b与各观察身份；最终883ad2c只补Pause失败传播单测/类型/构建，不冒充最后源全部实测。两个播放案例保留原报告通过记录，报告总体因后续paused验收报告计数错误为failed；只修为已确认暂停基线并补paused专项，已过案例不重跑，验收器修正不重编。实际browser153不同于旧154测量；旧300秒性能/声学保留原身份。当前同一mono目标的pre-master窗口，不证明独立参考声部/不同声道/post-master/DAC。unknown覆盖/scene/revision/原凭据等仍缺，S1及36组不标绿。现场句柄0、preview actor0|
|S1场景/修订/凭据当前切片|原真实accepted seek切场景5秒后仍playing；client中止回执只发旧gen Stop。已保留在途typed原请求/key，撤离或失败后一次4秒有界重放、校验原固定修订/场景/rate/owner/lease再Stop，不采用新owner或恢复声画；权威API拒绝不重放，新active不受退役结果影响。四旧源回归失败，最终42声音+5画面+1文档绑定+8会话共56独立用例、Web边界184与一次类型/生产构建过，0Rust/native/SDK。34唯一证据、57状态/36未完成保持。新Web7db7d00同现场一次串行5实际案例过：accepted seek切场景原key请求完全相同/Stop gen1和2、看历史/返当前、playing/paused原受限凭据真实revoke；各9–10捕获native PID/start退出、设备closed/Worker0、原head不变，真实preview403且已有cookie不回退owner|仅新增必要双场景小工程，共享原CAS资源/资产版本，原短/密集修订未改、无复制媒体或重导入；两枚实际撤权凭据均revoke/私密文件删除，不入库。真实5项全部绑定同最终源/构建，无验收器修正或已过案例重复。独立声部/双声道/post-master/DAC、external head/全场景操作、grant撤除恢复、持久unknown控制原key恢复/BFCache/kill/network/TTL及长/VFR/非单位速率仍缺。一次重放是退出best effort，失败仍由原授权/lease兜底，不冒充完整unknown交付；旧300秒性能/声学各保留身份。现场句柄0、preview actor0，S1及36组不标绿|
|S1独立声部/立体声/母带当前观察|原播放器、native混音/字节观察和Classic母带链不改；实际末级gain保留destination连接，另接仅输出零的有界Worklet。原reset token与Worker启动session绑定，最大8context/每个4096双声道帧；独立七基拟合440目标/880左参考/660右参考，Classic参数加Chromium补偿/最大声道静态曲线作为外部预期。9观察/负向用例、Web边界184及语法/注册过，0Rust/native/Web/SDK构建。复用原CAS，仅缺不同声道素材才新增一个Float32 stereo WAV/小复制工程；三个实际低峰值基线/−12dB/Escape恢复及一个高DC专项声学/像素过，两点共32768帧，最大成功偏差80ms。高前2/1、后0.971131/0.485566，专项deviceclosed/Worker0、原head/身份不变；35唯一证据、57状态/36未完成保持|低峰值3成功记录保留其观察身份和总体failed报告（后续错误读取旧seek窗口）；只修验收guard/负向用例并补高专项，不重跑已过案例/旧300秒或构建。首次真实高母带残差>0.0001仍开放：缺原窗口统计，后续同产品诊断通过不证明原因已修，不猜测欠载/压缩器原因或放宽阈值。最终观察保留失败前后统计/context身份；下一步补有界原窗口及实际量子回执关联。只有指定interior幅度/linked DC，不证明精确相位/边界/历史/DAC、持续性或导出等价；unknown/BFCache/network/TTL/external head/full controls及长/VFR/非单位速率仍缺。现场句柄0/preview actor0，S1及36组不标绿|
|B0有界失败捕获|新增与受影响观察15项、PCM/消费观察11项、Web边界188及语法通过；0产品构建。复用原声学工程/CAS、Rust/Web/audio；受控延迟原block10及下一credit直到30条实际零复制回执，最多2块。前后4096双声道帧及316真实量子记录保留132598B≤320KiB；母带前2/1，母带后3232零帧与缺帧精确对应、错位0，实测首输出延迟288帧，10独立像素/偏差69.33ms，原head/owner保持、deviceclosed/Worker0、现场句柄与native actor0|受控注入只证明诊断链，不解释或关闭首次自然母带残差。首次默认轮询错过短场景窗口，保留超时报告后只补采样间隔/异常原窗口保留并重跑受影响案例；未重复旧300秒/DSP或重建。视觉registry包含冻结时尚未接线B1文字模块，记录独立身份；不能宣称原visual hash或后来文字实现已验。B0结束，转B1；原问题等待新的真实失败窗口或相关产品变化再诊断，最终完整声音验收前仍须收口。|
|历史不可替代证据|旧F06产品77+恢复9及独立红像素/800Hz PCM、原生462排除Desktop；Legacy六像素；原生未母带八PCM；早期共享DSP整场母带；P0路径别名修复与CFR导出独立帧。各自限原候选，不推为当前全仓/整包通过|

## 待完成与卡点

|范围|必要剩余工作|
|---|---|
|S1基础|F08真实unknown视频、当前V4 VFR视频导出及更广格式矩阵；连续normal声画的更广源逐窗连续性、高母带残差定位、PCM精确边界/相位/历史、长源和操作矩阵；指定720p短窗口、300秒终态/区间/分列资源和20源/19间隙PCM interior已验，旧长尾未证明永久消失；完整元素/动画/锁轨/ripple命中矩阵；F17完整源/格式与长素材矩阵；F16补Finder/跨应用/原生视频音频粘贴、系统IME及更广格式/时长/失败组合；指定非整帧PCM/碰撞/失败恢复已验|
|S2文字字幕|合法字体bytes/license/固定face与glyph、独立文字全字段/布局/画布编辑、字幕语言/时间/内容等价；实施前置与分工见计划3.3|
|S3图形合成|图形/描边/动画、17混合、复杂路径蒙版/手柄/多蒙版动画、效果栈/轨道及背景|
|S4动画交互|过冲/Pop/预设、关键帧多选复制、速度/音高、全元素/轨道与实际内容拖动预览|
|S5/S6导出与收口|WebM/质量/音轨开关、完整取消发布窗口、线上资源许可、完整快捷键、真实公共数据/元数据、13页及X01–X03、冻结全量本机交付|

P0产物缺陷已修复：canonical `/private/var`发布与`/var`清理根别名导致误删，现规范化可信根，保留原租约/inode/下载保护；34受影响原生及原导出下载/独立帧通过。原故障丢失文件没有恢复。单场景CFR精确播放头、非零source/timeline origin、变化opacity已验；F08保持部分迁移。

容量阻塞已解除，账户/资源/反馈决策已确定，持久WAV发布/单帧normal/受限声音前置已验；F17当前缓存维护批已验且旧首次thumbnail原因仍未确认；通用V2四类fixed-head恢复已交付，F08当前V4跨场景与限定VFR导入已验；后续按计划3.4补真实unknown/VFR视频导出、更广格式、真实系统交互与连续声画剩余矩阵。S1及36项仍未全量交付。F18已核Kenney官方CC0，受限Freesound401仅是可选来源；目录/试听/获取/生成/放置尚未交付。F24 Classic Logos禁用且provider为空，不能伪造品牌来源；形状/国旗可独立推进。Google字体可访问不等于合法固定字体链交付。

更广R0–R7保留：指定硬件/长素材混格式、全工作区与多宿主故障、严格字体/插件/引擎恢复及全局事务、专业选片/打点/稳像/跟踪、完整色彩/音频/provider、跨平台安装/签名/升级。目标见[完整产品设计](agent-friendly-editor-v1.md)，当前迁移不关闭这些工作。

## 维护与资源治理

仅维护计划、台账和必要验收摘要，不积累逐轮hash清单/失败尝试/过程报告。此前契约治理压缩文档、按完整引用闭包去重OpenAPI并限制验收数据/复用构建；OpenAPI本次从3495769降至1713729字节，387组件语义与209路径保持，工具/SDK/event/Web四项产物字节不变。8项生成器测试、生成检查、22项旧协议检查及4项打包单测通过；合并重复Moon契约任务、补齐SDK测试缓存输入、复用打包工具清单。未重编Rust或运行真实媒体，该治理批不新增产品通过声明；本次F17维护独立按上表留证；57项状态、36项未完成集合、用户排除及独立像素/PCM/取消权限证据保持。

已识别的合成现场/数据库/媒体/log/临时报告、过期测试/CLI与无用增量构建在确认所属服务停止、进程退出和无打开文件后清理；当前必要产物/回归、Classic参考和用户媒体保留。后续用可复用脚本与小型TempDir合成夹具验证，实际结果绑定冻结候选，现场提取后删除。本批保留一个已停服的自有密集合成工作区供下一步收口，复用既有媒体/工程/修订；不入库或上传CI，用户媒体未触碰。
