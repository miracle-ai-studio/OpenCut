# Web 编辑台源码检查（2026-09-23）

恢复源码后，`apps/web` 起初没有已安装依赖，`npm --prefix apps/web test` 只返回 `vitest: command not found`。本机无 Bun；用 npm 11 在当前 macOS x86_64 / Node 24.19.0 环境安装依赖作源码验证，不把这次安装等同于仓库指定的 Bun 可复现发行。

安装后，`npm --prefix apps/web test` 的 6 个文件／31 项逻辑测试通过；`npm --prefix apps/web run build` 的 client 2,157 模块和 SSR 2,233 模块生产构建通过；`apps/web/node_modules/.bin/tsc --noEmit -p apps/web/tsconfig.json` 通过。构建产生的 `routeTree.gen.ts` 仅为生成顺序变化，已还原，未纳入源码修改。

`contracts:check` 首次失败，原因是 Web 的旧生成脚本直接导入未声明的 `json-schema-to-typescript`，而该依赖属于独立 SDK。Web 包装脚本现调用 SDK 的统一生成入口，`npm --prefix apps/web run contracts:check -- --binary <当前 opencut> --skip-build` 通过；生成的 Web 兼容导出、SDK 类型、75 路径 OpenAPI 和事件 Schema 使用同一个来源。

这些检查只证明当前源码的逻辑、类型、构建和生成契约。未通过真实浏览器点击、媒体播放、与当前服务共同运行的 UI 动线、Bun 工具链／锁定依赖或部署验收；R7 仍为部分完成。
