# Release Checklist

## 版本与文档

- [ ] `package.json`、`package-lock.json`、`Cargo.toml`、`Cargo.lock`、`tauri.conf.json` 版本一致
- [ ] README 功能矩阵与默认构建、`iroh-backend` 构建一致
- [ ] CHANGELOG/阶段文档没有把 stub 或实验功能描述为默认可用
- [ ] `LICENSE`、`LICENSE-MIT`、`LICENSE-APACHE`、包元数据和 README 均声明 `MIT OR Apache-2.0`
- [ ] 第三方许可证和 Kubo 版本信息完整
- [ ] 中文 `README.md` 与英文 `README_EN.md` 的功能矩阵、命令和版本信息一致

## 安全与供应链

- [ ] Windows Kubo 下载通过官方 SHA-512 sidecar 校验
- [ ] 仓库中不存在示例、占位或未经核验的“官方哈希”
- [ ] CID、MFS 路径、API/Gateway 地址和下载目标的命令边界测试通过
- [ ] 安装包不包含配置、日志、IPFS 仓库或密钥
- [ ] 检查 `npm audit` 与 Rust 依赖公告并记录接受的风险

## 验证

- [ ] `npm ci`
- [ ] `npm run typecheck`
- [ ] `npm test`
- [ ] `npm run build`
- [ ] `cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check`
- [ ] `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings -A deprecated`
- [ ] `cargo test --manifest-path src-tauri/Cargo.toml --lib`
- [ ] `cargo check --manifest-path src-tauri/Cargo.toml --features iroh-backend`
- [ ] 真实 Kubo 临时仓库 init → daemon → RPC → stop 集成测试通过
- [ ] benchmark 记录环境、预热、文件大小、迭代次数和原始样本

## 打包冒烟测试

- [ ] Windows 全新用户环境无需系统 Kubo，可自动初始化并启动
- [ ] 已有 `~/.ipfs` 不被覆盖
- [ ] `IPFS_GO_EXEC` 和 PATH 覆盖行为正常
- [ ] 关闭窗口后托盘行为正常，退出后子进程被清理
- [ ] 安装、升级和卸载均验证

## 发布

- [ ] Git tag 与应用版本一致
- [ ] 各平台构建产物名称和架构正确
- [ ] Release notes 明确 breaking changes、实验功能和已知问题
- [ ] 下载链接与校验信息可用
