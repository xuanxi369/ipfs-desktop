# IPFS Desktop (Rust Edition)

一个用 **Rust + Tauri 2 + React 18 + TypeScript** 编写的轻量 IPFS 桌面客户端，是官方
[ipfs/ipfs-desktop](https://github.com/ipfs/ipfs-desktop)（Electron）的重构尝试。

> **架构定位**：当前是「**Rust GUI + 进程管理**」驱动 **Go 版 Kubo 守护进程**
> （`Rust GUI → Kubo daemon`），而非 Rust 原生 IPFS 核心。性能收益主要来自
> GUI 层（Tauri vs Electron），节点层仍是 Kubo。项目内置统一的 `Backend`
> 抽象（[`backend_trait.rs`](src-tauri/src/backend_trait.rs)），为将来接入 Rust 原生后端（iroh）预留了扩展点，
> 但 **iroh 后端目前仅是 stub，尚未实现实际文件/Pin/IPNS 操作**。
>
> 关于「为什么这样做」以及长期技术路线，见 [项目路线.md](项目路线.md)。

---

## 功能现状

后端已实现 **40 个 Tauri 命令**，前端为单页多标签界面（控制面板 / WebUI / 文件 / Pin / IPNS）。

| 能力 | 状态 | 说明 |
|------|------|------|
| 守护进程控制（启动/停止/重启） | ✅ | 子进程生命周期管理，SIGTERM→轮询→SIGKILL，5s 健康监控，`Drop` 兜底清理 |
| 节点仪表盘（peers/带宽/bitswap/repo） | ✅ | 10s 自动轮询（`tokio::join!` 并行拉取）+ SQLite 缓存 + 事件推送 |
| 文件上传 / 下载 / 预览 | ✅ | 上传与下载均**流式**（分块，不整文件驻留内存）+ 真实进度条 |
| Pin 管理（列表/添加/移除） | ✅ | 写操作后缓存自动失效 |
| IPNS 发布 / 解析 + 密钥管理 | ✅ | **密钥由 Kubo 密钥库权威管理**；本应用只保存「标签→真实 IPNS 名」的公开记录，不接触私钥 |
| 智能代理（缓存 + 熔断 + 预取 + 指标） | ✅ | 读命令统一走 `ProxyClient`，缓存命中率/延迟/熔断统计一致化 |
| 离线操作队列 + 自动重放 | ✅ | 守护进程恢复后每 15s FIFO 重放，最多重试 3 次 |
| 带宽 / 连接数配置 | ✅ | 30 点滑动窗口速率平滑 |
| 开机自启 / 系统托盘 | ✅ | |
| 查询参数安全 | ✅ | 所有用户输入参数经 URL 百分号编码 |
| Kubo vs iroh 基准 & 兼容性测试 | ⚠️ | 框架已接通；iroh 侧为 stub，对比数据仅供参考 |
| **iroh 原生后端** | 🚧 | stub，仅 `node_info` / `version` 可用，UI 暂不开放切换 |

> **安全说明**：私钥全程由 Kubo 保管，本应用代码不生成、不存储、不经 IPC 传输任何私钥。
> 早期"本地明文私钥落盘"的实现已移除。二进制查找目前仅做「行为验证」（能否输出 Kubo 版本字符串），
> **不做加密签名/哈希校验**——如需防篡改请自行比对官方发行版哈希。

---

## 环境要求

- **Rust** 1.70+（建议用 [rustup](https://rustup.rs/)）
- **Node.js** 18+ 与 npm
- **Kubo (go-ipfs)**：需自行安装 [Kubo](https://github.com/ipfs/kubo)。应用按以下顺序自动发现二进制：
  1. 环境变量 `IPFS_GO_EXEC` 指向的路径
  2. 系统 `PATH` 中的 `ipfs`（Windows 为 `ipfs.exe`）
  3. 与应用同目录 / `bin/` / `resources/` 下的内置二进制
- 平台：Windows / macOS / Linux
- Tauri 2 的系统依赖（Linux 需 `webkit2gtk` 等），见 [Tauri 前置要求](https://tauri.app/start/prerequisites/)

首次准备好 Kubo 后，建议初始化一次仓库：

```bash
ipfs init
```

---

## 快速开始

```bash
# 1. 安装前端依赖
npm install

# 2. 开发模式（前端热更新 + Rust 后端，自动打开窗口）
npm run tauri dev

# 3. 生产构建（产出各平台安装包到 src-tauri/target/release/bundle/）
npm run tauri build
```

启动应用后，在「控制面板」标签点击 **Start Daemon** 拉起 Kubo；随后即可使用仪表盘、文件、Pin、IPNS 等功能。

---

## 运行与调试

### 后端（Rust / Tauri）

```bash
# 快速编译检查（不产出二进制，最快的正确性反馈）
cargo check --manifest-path src-tauri/Cargo.toml

# 运行全部单元测试（部分集成测试在检测到本机 Kubo 时才实跑，否则自动跳过）
cargo test --manifest-path src-tauri/Cargo.toml --lib

# 代码质量（CI 同款）
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings -A deprecated

# 格式化
cargo fmt --manifest-path src-tauri/Cargo.toml
```

**日志与调试**：应用使用 `tracing` 输出到**控制台**与**滚动文件**（每日一个 `app.log`）。

- 日志文件目录：`<系统本地数据目录>/ipfs-desktop-rust/logs/`
  - Windows：`%LOCALAPPDATA%\ipfs-desktop-rust\logs\`
  - macOS：`~/Library/Application Support/ipfs-desktop-rust/logs/`
  - Linux：`~/.local/share/ipfs-desktop-rust/logs/`
- 通过 `RUST_LOG` 调整级别（默认 `info`）：

```bash
# Linux / macOS：更详细的调试日志
RUST_LOG=debug npm run tauri dev
```

```powershell
# Windows PowerShell：设置日志级别后启动
$env:RUST_LOG="debug"; npm run tauri dev
```

**指定自定义 Kubo 二进制**（跳过 PATH 查找）：

```bash
IPFS_GO_EXEC=/path/to/ipfs npm run tauri dev
```

### 前端（React / TypeScript）

```bash
# 类型检查（无输出即通过）
npm run typecheck

# 仅启动前端 Vite 开发服务器（不含 Rust 后端；invoke 调用会失败，仅用于纯 UI 调试）
npm run dev
```

### 应用内数据位置（调试时可清理）

| 数据 | 路径 |
|------|------|
| 配置 | `<系统配置目录>/ipfs-desktop-rust/config.json` |
| SQLite 缓存 | `<系统本地数据目录>/ipfs-desktop-rust/cache.db` |
| 离线队列 | `<系统本地数据目录>/ipfs-desktop-rust/offline_queue.db` |
| 密钥公开记录 | `<系统本地数据目录>/ipfs-desktop-rust/keys/*.json`（**不含私钥**） |
| 日志 | `<系统本地数据目录>/ipfs-desktop-rust/logs/app.log` |

---

## 项目结构

```
ipfs-desktop-rust/
├── src-tauri/                  # Rust 后端（约 7000 行）
│   ├── src/
│   │   ├── lib.rs              # 入口：插件注册 + 40 命令注册 + 日志初始化 + 托盘
│   │   ├── main.rs            # 程序入口
│   │   ├── commands.rs        # 40 个 #[tauri::command]（所有状态变更的唯一入口）
│   │   ├── state.rs            # 全局状态 AppState（Arc<RwLock>）+ 健康监控 / 轮询 / 重放
│   │   ├── config.rs           # JSON 配置加载/保存/校验
│   │   ├── error.rs            # 统一 DaemonError（thiserror + Serialize 传前端）
│   │   ├── types.rs            # DaemonStatus / AddResult 等数据类型
│   │   ├── tray.rs            # 系统托盘（Show/Hide/Quit）
│   │   ├── cache.rs            # SQLite TTL 缓存（6 类数据，10s–300s）
│   │   ├── keyring.rs          # 密钥「公开记录」仓库（不含私钥，Kubo 为权威）
│   │   ├── proxy.rs            # 智能代理：缓存路由 + 熔断器 + 预取 + 指标
│   │   ├── offline_queue.rs    # SQLite 离线操作队列 + ReplayEngine
│   │   ├── bandwidth.rs        # 带宽/连接数配置 + 速率平滑
│   │   ├── backend_trait.rs    # 统一 Backend trait（16 个 async 方法）★ 核心抽象缝
│   │   ├── kubo_adapter.rs     # Kubo → Backend 适配器（完整实现）
│   │   ├── iroh_adapter.rs     # iroh → Backend 适配器（stub + 真实实现模板）
│   │   ├── compat_test.rs      # 协议兼容性测试框架
│   │   ├── benchmark.rs        # 性能基准测试框架
│   │   └── daemon/             # 二进制查找 / 进程控制 / HTTP API 客户端（19 端点）
│   ├── capabilities/          # Tauri 权限能力声明
│   ├── Cargo.toml
│   └── tauri.conf.json
├── src/                        # React 前端
│   ├── App.tsx                 # 主界面（5 Tab + 进度条 + 后端选择器）
│   ├── i18n.ts, locales/       # 中英文国际化
│   └── ...
├── .github/workflows/build.yml # CI：三平台 typecheck + cargo check + test + clippy
├── 项目路线.md                 # 长期技术路线（研究性）
└── package.json
```

---

## CI / 发布

`.github/workflows/build.yml` 在 **Linux / macOS / Windows** 三平台执行：前端类型检查、`cargo check`、单元测试与 clippy。
打 `v*` tag 时构建并发布各平台安装包。

---

## 路线图（概要）

详见 [项目路线.md](项目路线.md)。核心思路是**沿 `Backend` 抽象缝渐进迁移节点**，而非推倒重写：

1. **夯实 Kubo 锚点**：补全 MFS 等能力，作为长期兜底后端。
2. **iroh 实装**：把 `iroh_adapter.rs` 从 stub 补全，让双后端真正可切换，用 `benchmark.rs` 产出真实对比。
3. **Rust 原生成为默认快车道**：本地/信任圈走 iroh，公网互操作走 Kubo（双栈路由）。
4. **可信个人节点**：去中心化身份、加密存储、长期在线可用性。

---

## License

MIT

## 相关资源

- [Tauri 文档](https://tauri.app/)
- [IPFS Desktop（原版）](https://github.com/ipfs/ipfs-desktop)
- [Kubo (go-ipfs)](https://github.com/ipfs/kubo)
- [iroh](https://github.com/n0-computer/iroh)
