# opencut-sdk-rust

OpenCut 的阻塞式类型化 HTTP 客户端。客户端不拥有工程或任务状态，不刷新修订，也不自动重试未知结果的写入。

```rust
use opencut_sdk_rust::{Client, api};
let client = Client::new("http://127.0.0.1:8788", token)?;
let capabilities = client.host_capabilities()?;
```

`call::<api::tools::Tool>()` 使用 Rust 注册声明的输入/输出，`call_envelope` 另保留 request_id 和 warnings。公共错误包括配置、传输、协议、API 拒绝、等待超时、观察取消及任务终态；超时或停止观察不取消宿主任务。

任务通过公开 JobView 与 `job_result` 读取类型化结果。媒体上传、下载和 PCM 流遵守宿主发布的授权、长度、哈希、范围与取消约束。客户端不能从私有存储路径取资源，也不能把能力发现视为执行授权。

```sh
cargo test -p opencut-sdk-rust --locked
```

集成检查使用显式服务和合成工作区，CLI 与 HTTP/SDK 共享固定修订、原请求、幂等事务和权限语义。源码包保留实际编译模块、许可证及回归测试；完整平台与产品限制见根 README。
