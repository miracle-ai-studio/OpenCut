# 上传媒体检查与元数据修复契约

状态：已实现共享 CLI／MCP／HTTP 入口。版本：v1。日期：2026-09-21。

## 作用域

修复工作区上传媒体目录中的类型、时长、MIME 和探测字段。不会修改媒体字节、CAS 版本、任何工程修订或历史任务结果。已经把错误类型绑定进工程的情况，需要客户端使用返回的 asset 显式规划并提交工程修正；不能把目录修复解释为旧工程已自动修复。

两个入口使用相同 Automation 用例：

| 操作 | 输入 | 结果 | 性质 |
|---|---|---|---|
| `opencut_media_inspect` | `content_hash` | `CatalogEntry` | 读取存储的元数据；不声称当前媒体字节完整 |
| `opencut_media_repair` | `content_hash`, `expected_metadata_hash` | 修复后的 `CatalogEntry` | 校验媒体并比较版本后原子替换元数据 |

`CatalogEntry` 包含 `content_hash`、`metadata_hash: string|null`、`asset: Asset|null`。metadata_hash 是磁盘上原始 JSON 字节的 SHA-256，不是媒体 hash，也不是工程 revision。即使 JSON 无法解析，也能返回修复令牌；缺失元数据时为 null。

内容标识必须是 64 位小写十六进制 SHA-256。请求不接受任意文件路径。`expected_metadata_hash` 为 null 只允许修复当前缺失的元数据；若已有文件则返回冲突。

## 调用示例

将真实 content hash 写入 `inspect.json`：

```json
{"content_hash":"<64位内容哈希>"}
```

```sh
opencut --database /path/to/workspace.sqlite --json call opencut_media_inspect --input inspect.json
```

将观察到的 metadata_hash 原样写入 `repair.json`：

```json
{"content_hash":"<64位内容哈希>","expected_metadata_hash":"<inspect返回的元数据哈希>"}
```

```sh
opencut --database /path/to/workspace.sqlite --json call opencut_media_repair --input repair.json
```

MCP 使用同名工具和相同参数。HTTP 使用带现有 Bearer 授权的 `POST /v1/tools/opencut_media_inspect` 与 `POST /v1/tools/opencut_media_repair`，请求体不另包一层 arguments。

## 并发、恢复与错误

- 上传与修复使用同一按内容标识划分的文件锁，不持有工程数据库写事务。完整 JSON 写入并同步后原子发布；两个调用使用同一个旧令牌修复错误元数据，只能一个成功，另一个收到冲突。
- 修复对私有媒体副本计算 hash，再探测同一份已验证字节。媒体身份不匹配时不发布新元数据。修复过程无需猜测原始扩展名。
- `MEDIA_METADATA_CONFLICT`：HTTP 409、CLI 5，details 提供内容标识、版本信息与下一步检查操作。客户端重新 inspect 并判断，不盲目替换期望版本重试。
- `ASSET_INTEGRITY_ERROR`：HTTP 500、CLI 8。应恢复正确媒体字节；修复元数据不能掩盖内容损坏。
- 调用中断导致结果未知时，先 inspect 读取当前元数据和令牌；修复不承诺使用旧令牌重复调用始终成功。
- 元数据缺失／损坏可以修复；源字节缺失、损坏、无法识别和缺探测工具返回错误。本地目录权限及旧版本跨平台迁移需要独立验证。

## 架构边界

`asset-store::catalog` 管理锁、版本比较、内容校验、私有副本及原子存储，探测由回调提供，不依赖 FFmpeg、HTTP 或 SQLite。`automation::media` 把媒体后端探测结果转换为工程 Asset。HTTP 上传与修复共享该转换，避免再次按扩展名分叉。

这是 R2 的部分边界收拢，不代表所有 Automation 编排、独立 SDK 或公共 DTO 的后端隔离已经完成。

## 容器品牌与 MIME

上传类别只由内容探测的流类型和容器信息决定；调用方提供的文件名只用于显示，不用来推断资产 kind 或 `mime_type`。ISO Base Media 格式族在 FFprobe 的 `format_name` 中共享 MOV/MP4/3GP/3G2/MJ2 别名，因此 `MediaMetadata.format_brand` 可选地保存 `major_brand`，例如 `qt` 区分 QuickTime、`isom` 表示常见 ISO MP4、`3gp*`／`3g2*` 区分 3GPP；品牌按视频或纯音频流映射到相应 MIME。该字段也由 `media.probe` 的公共 Job 视图输出；旧结果没有它时保持兼容读取。

Matroska/WebM 使用共享 FFprobe demuxer，不能仅凭该别名稳定区分容器品牌，当前上传元数据保守使用 `application/octet-stream`。其他未知 brand 不从扩展名猜测。已声明上传探测白名单的容器族由公共 HTTP 格式矩阵验证 MIME 和下载字节身份。
