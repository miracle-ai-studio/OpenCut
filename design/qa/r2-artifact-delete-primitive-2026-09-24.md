# R2 已发布产物精确删除原语增量验收（2026-09-24）

状态：**原生删除围栏通过，公开产物保留／回收尚未完成。** 本轮将下载和清理共用的尝试目录 inode 锁扩展为 `ArtifactCandidate::observe/remove_if`。候选记录尝试目录与普通单链接文件的身份；执行非阻塞获取独占锁，在锁内调用引用重查回调，并对身份未变的直接子文件 `unlinkat`、同步目录。活跃下载、文件或尝试目录替换均不删除；盘点出引用或引用查询报错时也不删除。源文件打开增加 `O_NONBLOCK`，防止恶意命名管道在文件类型验证前卡住宿主。

`cargo test -p opencut-native-services -p opencut-storage-budget` 通过；其中原生测试覆盖活跃读取与删除互斥、重试已删除项、文件替换与符号链接、目录替换但原文件 inode 不变、命名管道拒绝。`cargo clippy -p opencut-native-services -p opencut-storage-budget --all-targets -- -D warnings` 及格式检查通过。最新 CLI debug 二进制 SHA-256 为 `5fa61f4a072efc18a4a8029b15f560d06e855a5c829bf27a2682e66362c2170a`；其真实 HTTP／CLI [10 项媒体产品动线](evidence/r2-artifact-delete-primitive-product-2026-09-24.json)通过，包含下载、篡改与别名拒绝及恢复。媒体样本为合成音视频。

完整测试首次复跑时，尝试目录锁测试在并行 FFmpeg 子进程启动窗口偶发拿不到独占锁；单项 100 次复现均通过，修正前并行测试组 20 次出现 2 次。测试现按真实非阻塞协议有界重试并保留取得的独占 guard，不再把瞬时 busy 当作永久失败；完整套件及修正后并行测试组 20 次均通过。此修正只改变测试等待逻辑，不改变生产锁协议。

当前没有调用该删除原语的公开用例、release 标记、持久计划／回执或完整引用重查实现；因此**任何已发布产物都不会因本轮新增代码被 GC 删除**。本轮未制作新的交付归档，旧 35 项整包收据只适用于此前归档。完整协议与后续验收门槛见[产物保留契约](../architecture/artifact-retention-contract.md)。
