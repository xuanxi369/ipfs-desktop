# IPFS Desktop (Rust Edition)

一个用 **Rust + Tauri 2 + React 18** 重写的轻量 IPFS 桌面客户端,是官方
[ipfs/ipfs-desktop](https://github.com/ipfs/ipfs-desktop)(Electron)的重构尝试。

> **架构定位**:目前是 “Rust GUI + 进程管理” 驱动 **Go 版 Kubo 守护进程**
> (`Rust GUI → Kubo daemon`),而非 Rust 原生 IPFS 核心。性能收益主要来自
> GUI 层(Tauri vs Electron),节点层仍是 Kubo。项目内置了统一的 `Backend`
> 抽象(`backend_trait.rs`),为将来接入 Rust 原生后端(iroh)预留了扩展点,
> 但 **iroh 后端目前仅是 stub,尚未实现实际操作**。

## 功能现状

后端已实现约 40 个 Tauri 命令,前端为单页多标签界面(控制面板 / WebUI / 文件 / Pin / IPNS)。

| 能力 | 状态 | 说明 |
|------|------|------|
| 守护进程控制(启动/停止/重启) | ✅ | 子进程生命周期管理,SIGTERM→SIGKILL,健康监控 |
| 节点仪表盘(peers/带宽/bitswap/repo) | ✅ | 10s 自动轮询 + SQLite 缓存 + 事件推送 |
| 文件上传 / 下载 / 预览(带进度) | ✅ | |
| Pin 管理(列表/添加/移除) | ✅ | 写操作后缓存自动失效 |
| IPNS 发布 / 解析 + 密钥管理 | ✅ | Ed25519,密钥经系统 keyring 存储 |
| 智能代理(缓存 + 熔断 + 预取 + 指标) | ✅ | 读命令经 `ProxyClient` 统一走缓存/熔断 |
| 离线操作队列 + 自动重放 | ✅ | 守护进程恢复后每 15s 重放 |
| 带宽 / 连接数配置 | ✅ | |
| 开机自启 / 自动更新 / 系统托盘 | ✅ | |
| Kubo vs iroh 基准测试 & 兼容性测试 | ⚠️ | 已接通;iroh 侧为 stub,对比数据仅供参考 |
| **iroh 原生后端** | 🚧 | stub,仅 `node_info`/`version` 可用,UI 暂不开放切换 |

## 环境要求

- **Rust** 1.70+(建议用 `rustup`)
- **Node.js** 18+ 与 npm
- **Kubo(go-ipfs)**:通过环境变量 `IPFS_GO_EXEC`、系统 `PATH` 中的 `ipfs`,
  或与应用同目录的内置二进制被自动发现。请先安装 [Kubo](https://github.com/ipfs/kubo)。
- 平台:Windows / macOS / Linux

## 快速开始

```bash
# 安装依赖
npm install

# 开发模式(前端热更新 + Rust 后端)
npm run tauri dev

# 生产构建
npm run tauri build
```

## 项目结构

```
ipfs-desktop-rust/
├── src-tauri/                  # Rust 后端(约 6900 行)
│   ├── src/
│   │   ├── lib.rs              # 入口:插件注册 + 命令注册 + 托盘
│   │   ├── commands.rs         # ~40 个 Tauri 命令
│   │   ├── state.rs            # 全局状态(Arc<RwLock>)+ 后台任务
│   │   ├── config.rs           # 配置加载/保存
│   │   ├── cache.rs            # SQLite TTL 缓存
│   │   ├── proxy.rs            # 智能代理(缓存 + 熔断 + 预取)
│   │   ├── offline_queue.rs    # 离线操作队列 + 重放
│   │   ├── bandwidth.rs        # 带宽/连接数配置
│   │   ├── keyring.rs          # Ed25519 密钥 + 系统 keyring
│   │   ├── backend_trait.rs    # 统一后端抽象
│   │   ├── kubo_adapter.rs     # Kubo(HTTP)后端实现
│   │   ├── iroh_adapter.rs     # iroh 后端(stub)
│   │   ├── benchmark.rs        # Kubo vs iroh 基准测试
│   │   ├── compat_test.rs      # 协议兼容性测试
│   │   └── daemon/             # 二进制查找 / 进程控制 / HTTP API 客户端
│   └── Cargo.toml
├── src/                        # React 前端
│   ├── App.tsx                 # 主界面(多标签)
│   ├── i18n.ts, locales/       # 中英文
│   └── ...
└── package.json
```

## 开发与测试

```bash
# 前端类型检查
npm run typecheck

# Rust 单元测试(部分集成测试需要本机已安装并运行 Kubo)
cargo test --manifest-path src-tauri/Cargo.toml --lib

# 代码质量
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings -A deprecated
```

CI(`.github/workflows/build.yml`)在 Linux/macOS/Windows 三平台上执行类型检查、
`cargo check`、单元测试与 clippy;打 `v*` tag 时构建并发布安装包。

## 路线图

1. **接入 Rust 原生后端**:把 `iroh_adapter.rs` 从 stub 补全为真实实现(启用 `iroh-backend` feature),
   让 `Backend` 抽象真正做到 Kubo/iroh 可切换。
2. 逐步用 Rust libp2p 替代 Go Kubo 子进程,走向 “Rust 原生节点”。
3. 去中心化身份 / 密钥 / 加密存储等个人节点能力。

## License

MIT

## 相关资源

- [Tauri 文档](https://tauri.app/)
- [IPFS Desktop(原版)](https://github.com/ipfs/ipfs-desktop)
- [Kubo(go-ipfs)](https://github.com/ipfs/kubo)
