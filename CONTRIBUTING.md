# Contributing

[简体中文](CONTRIBUTING.md) | [English README](README_EN.md)

感谢参与 IPFS Desktop Rust。

## 开发环境

1. 安装 Node.js 20、Rust stable 和 Tauri 2 平台依赖。
2. 运行 `npm ci`。
3. Windows 开发内置 Kubo 时运行 `npm run setup:kubo`。
4. 使用 `npm run tauri dev` 启动默认 Kubo 构建。

实验性 iroh 构建：

```bash
npm run tauri dev -- --features iroh-backend
```

## 提交前检查

```bash
npm run typecheck
npm test
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings -A deprecated
cargo test --manifest-path src-tauri/Cargo.toml --lib
```

新增 Tauri 命令时必须在命令入口验证不可信输入，并在 `src-tauri/src/lib.rs` 注册。涉及 Kubo JSON 的结构应使用 `serde(alias = "KuboField")` 接受上游字段，同时保持 IPC 输出为 snake_case。

## 测试约定

- 文件系统测试使用 `tempfile`，不得写入用户真实 `.ipfs` 或应用数据目录。
- 本机 HTTP 测试不得继承系统代理。
- 需要真实 Kubo 的测试必须使用独立临时仓库，并在 Kubo 不可用时明确跳过。
- 修复缺陷时应添加能复现该缺陷的回归测试。

## Pull Request

PR 应说明行为变化、风险、验证命令和默认构建/iroh feature 的影响。不要提交 `node_modules`、`dist`、`target`、下载的 `ipfs.exe`、用户配置、日志或密钥材料。

## 贡献许可

除非您明确声明，否则您有意提交并纳入本项目的贡献将按 `MIT OR Apache-2.0` 双重许可，不附加额外条款或条件。
