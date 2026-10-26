# 开发与检查

安装、构建和启动见[README](../README.md)。Python 脚本要求 Python 3.11+。Rust 使用根 Cargo.lock，JavaScript 包使用各自 npm 锁文件；生成契约保持可审阅格式。

## 边界与行为

仓库不包含 GitHub Actions 流水线。按实际改动范围在本地运行以下检查。

```sh
python3 -m unittest discover -s scripts/tests -p 'test_architecture.py'
python3 scripts/check-architecture.py
python3 scripts/check-contract-boundaries.py
python3 scripts/check-source-hygiene.py
npm run boundaries:check --prefix packages/sdk-ts
npm run boundaries:check --prefix apps/web
cargo test --workspace --exclude opencut-desktop --locked
python3 -m unittest discover -s scripts/tests
python3 -m unittest discover -s scripts -p 'test_acceptance_runner.py'
npm test --prefix packages/sdk-ts
npm test --prefix apps/web
npm test --prefix packages/classic-audio-renderer
npm test --prefix packages/classic-visual-renderer
npm test --prefix packages/classic-source-art-renderer
```

按实际改动选择检查，分别报告编译、静态门禁、单元、浏览器与完整验收。文档更新不要求渲染媒体或构建 Rust。有状态浏览器和媒体检查串行使用同一自有合成工作区，复用原请求、Job 和成功产物。

## 生成契约

代码集成后构建一次，使用同一显式二进制检查 SDK、OpenAPI、工具、领域与 Web 类型：

```sh
cargo build --locked --release -p opencut-cli
node packages/sdk-ts/scripts/generate.mjs --check --skip-build --binary "$PWD/target/release/opencut"
python3 scripts/check-canonical-contract.py --binary "$PWD/target/release/opencut"
```

不复制 DTO，不修改依赖基线绕过失败。验证器修正且产品代码未变时继续使用原二进制。

## 浏览器与打包

查看 `scripts/verify-web.py --help`，显式指定 CLI 和网页目录。素材使用合成或获准的公开样本，测试现场放在仓库外。真实 Finder、系统 IME、物理音频、人工审片和系统保存另作人工检查。

```sh
python3 scripts/package-local.py --help
python3 scripts/verify-relocated-runtime.py --help
python3 scripts/verify-package-exchange.py --help
```

包内程序、Web、SDK、运行时与许可证必须绑定准确身份。构建成功不等于完整发行验收；解包、搬迁、签名、安装与升级各自验证，见[交付状态](status.md)。

需要独立 Classic 源码作为预期值的专项检查，通过 `OPENCUT_CLASSIC_SOURCE_ROOT` 指定仓库外固定对照源码的绝对路径。该路径不是产品运行依赖；测试继续校验实际源码内容与来源身份。字体目录生成使用仓库内随产品交付的固定字体 atlas。
