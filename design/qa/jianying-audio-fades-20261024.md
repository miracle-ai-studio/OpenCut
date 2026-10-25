# 音频淡入淡出参数

图07的淡入淡出及直接撤销/重做面板修复，由独立sol6.1/high限定验收通过，见`review/sol-fades-review.md`、`sol-fades-validation-review.md`、`sol-history-tab-review.md`。原七图42项出口保留；本切片不代表单行试听、真实目录时长/波形、简洁音量或整个图07已完成。证据根目录 `/private/tmp/opencut-jianying-audio-ui-20261024`，当前候选 `candidate-history-tab.json`，真实一次保存/采样/Undo仍绑定首个候选 `candidate-fades.json`，5173实际源码UI；最终13源码、284份生产产物留存，未安装59790。Native仍b3f2a786，同实例未重建/重启，无公开契约或依赖变化。

## 实现与复用

右侧显示淡入、淡出秒数。只识别既有hold外推、无Bezier/handle且最多四点的线性包络；保留原dB或linear_legacy单位、峰值和片段本地时间。其它设置明确提示使用原音量/曲线控件，不猜两段时长，不把自定义曲线拟合成淡入淡出。无包络的非静音基础音量可以通过同一channel_put创建普通关键帧。零时长表示无斜坡；两段可相接，总时长不能超过片段。dB端点为−60dB，并非静音。标准化时可能合并冗余平台点，不能声称每个key的ID、分数表示或tangent_mode逐项不变。

数字确认仅提交该字段，Esc仅还原该字段；未改值不发请求。共享opencut_keyframe_plan规划后严格比对完整原目标element及仅目标gain channel变化，再经原applyPlanned/恢复协调器提交。API、认证身份、工程、修订、场景、选择、锁轨、播放及未确认编辑均有围栏；不在浏览器采样或另建音频乘数。保存后对已确认的新修订恢复原输入焦点。能力读取和保存的过期错误只交给原连接的统一断连处理。

## 检查与实际证据

`fade-tests-initial.log`三文件38项通过；静态审查发现极小负数量化后漏拒绝，以及能力读取未统一处理会话过期，已修复并增加针对检查。`fade-tests-reviewed.log`三文件43项通过，有重叠不相加；`fade-build.log`包含最终类型检查与生产构建通过，保留大chunk提示。`fade-boundaries.log`291文件通过。已见泛提示修复后的两文件32项及包含类型检查的生产构建另通过`fade-tests-validation.log`/`fade-build-validation.log`，与43项有重叠不相加；只增加本地错误类型和呈现，合法包络计算/原提交语义保留，后续独立复验以新候选为准。测试只证明输入、围栏和回执处理，不是媒体PCM或整个工作台验收。

`review/sol-fades-browser.json`及三尺寸截图证明1280×720、1440×900、1920×1080字段112×30px、中心及四边命中、页面无横纵溢出，原1.52秒/3秒真实显示。Esc及未改Enter零规划/零写；负值和重叠输入均零写。原重叠错误主提示泛化已实际发现，现以私有FadeInputError将本地负值/非有限/重叠错误直接显示为中文，并绑定错误字段aria-invalid/aria-describedby；服务器或网络错误仍走原ErrorNotice。独立`sol-fades-validation-review.md`已通过该零写复验：三条具体中文提示、错误字段关联和Esc清错还原通过，没有plan/write，不重提已成功的事务。

实际唯一保存：原QA125的hainan-clip-asset-music-0，淡出草稿2秒未确认，淡入1.52→0.5秒Enter保存；新QA126/head251e47ca的淡出仍3秒，键盘焦点回淡入字段。Native plan request cd03942f-abae-458b-abfd-3a311ab2717d，事务txn_012039527d4942f3be0edf23480f318d。主代理从SQLite固定新修订独立比较：恢复该元素channels后，整个项目文档与QA125完全相等，见`main-applied-proof.json`；原channel ID和linear_legacy峰值1.6921544822993657保留，淡入60000/120000、淡出42−39秒。验收器曾等待不存在的edit_recovery_apply而超时，实际为edit_apply且已成功，不作产品保存失败，不重提写入。

固定修订六点Native采样request d2f127bf-1204-4168-9f62-3a2ec2855dd3与独立预期最大误差0，见`main-fade-sample-oracle.json`及`main-sample-proof.json`。实际工具栏Undo为txn_0a23a68ec93840bf95a6dcfcb03392d0，SQLite请求kind为project_v4_selective_undo，明确引用唯一原事务，expected_revision等于forward修订；QA127/head14ff7afe/全文SHA768641c5精确回基线，用户49全文/head/事务不变，见`main-undo-proof.json`及`main-causal-undo-proof.json`。原Undo等待15秒找不到可访问字段，不能用全DOM隐藏字段1.52/3判定界面恢复。主代理实际查看`sol-fades-undo-restored.png`确认右侧历史tab高亮；源码对所有临时historyPlan自动切历史，属性panel被hidden。此根因已修：内部HistoryRequest的review意图区分直接执行与显式查看，直接无冲突Undo/Redo保持当前面板，显式查看或冲突仍进入历史。标记不进入Native请求；31项Session/布局/历史检查、类型构建及291文件边界通过，日志为`history-tab-*`。旧超时与错误归因保留并纠正，采样只证明共享authoring curve，不代表输出PCM、变速/修剪全部时域或独立导出。

`review/sol-history-tab-review.md`完成受影响实际补验：复用原undo链，真实Redo在1718ms内可访问0.5/3，Undo在1879ms内可访问1.52/3，均在原15秒门槛内。Properties始终选中、panelHidden=false、没有hidden祖先，DOM observer未见History抢占；显式进入History及返回Properties仍正常。只增加这两个必要历史动作，不重复淡入保存或六点采样。Redo txn_b0cad708a45f4a308280fa78bb7fb8f9引用旧undo0a23；新Undo txn_ba7f1d00d3054c4b9c9d999390ef2dee按共享navigation撤销被重激活的原forward012039，expected_revision为新Redo head，不虚称直接引用b0cad。主代理SQLite独立复核`main-history-tab-proof.json`：本阶段+2事务，整切片125→129共+4；QA129/head1f430664/全文SHA768641c5精确恢复，用户49全文/head/事务保持。

首段实际7670响应（7661×200、4×204、3×404、2×409），7627个唯一request_id查询拒绝表零匹配；零pageerror/console/401/403/500，21个ERR_ABORTED独立保留。404为缓存探测，409为已停止旧资源释放，原body/ID不抹去；尾5秒323响应含1缓存404。主代理全文日志仍104175字节/SHA1fa5eb0c，denial96343、failed20957、721终态Job未增。零写提示复验773响应（767×200、4×204、2缓存404）、748个唯一request_id零拒绝，16个取消保留，零pageerror/console/401/403/500；QA127/用户49、Job及日志保持，见`main-validation-request-audit.json`。

最终面板修复段4771响应（4761×200、4×204、4缓存404、2旧释放409），4732个request_id在拒绝表零匹配，18个取消保留，零pageerror/console/401/403/500；尾5秒334响应、2个取消。主代理日志全文仍104175字节/SHA1fa5eb0c，拒绝96343、失败20957、721终态Job未增。`service-status-final.json`实际ready=true、同b3/实例/PID78673；`main-final-candidate-proof.json`13源码/284产物精确匹配。主代理实际查看`sol-history-tab-return-properties.png`确认可见属性，不以隐藏DOM代替实际UI。observer及五类监听移除，runner80067退出；原Chrome/QA129、1200×959、无选/时间0/声音停止保留。前两段故障与取消仍保留各自来源，不宣称零请求失败。

## 保留范围

完整自定义曲线、历史/锁轨/异常恢复主要由静态及单测覆盖，本轮不造新工程或改变轨道制造状态。未确认的其它字段草稿目前随修订刷新重置，全草稿阻止拖放仍属原矩阵开放项。单行目录试听、真实时长/波形、音乐/收藏语义及简洁音量布局继续实施；冷起播和原Native源图预算拒绝未关闭。不得把本参数切片提升为七图整体验收。
