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

后端已实现 **55 个 Tauri 命令**（含 iroh 原生收发 5 个、iroh 生命周期/keep 3 个、双栈路由 3 个、节点身份 3 个、节点健康 1 个），前端为单页多标签界面（控制面板 / WebUI / 文件 / Pin / IPNS / iroh 原生）。

| 能力 | 状态 | 说明 |
|------|------|------|
| 守护进程控制（启动/停止/重启） | ✅ | 子进程生命周期管理，SIGTERM→轮询→SIGKILL，5s 健康监控，`Drop` 兜底清理 |
| 节点仪表盘（peers/带宽/bitswap/repo） | ✅ | 10s 自动轮询（`tokio::join!` 并行拉取）+ SQLite 缓存 + 事件推送 |
| 文件上传 / 下载 / 预览 | ✅ | 上传与下载均**流式**（分块，不整文件驻留内存）+ 真实进度条 |
| Pin 管理（列表/添加/移除） | ✅ | 写操作后缓存自动失效 |
| IPNS 发布 / 解析 + 密钥管理 | ✅ | **密钥由 Kubo 密钥库权威管理**；本应用只保存「标签→真实 IPNS 名」的公开记录，不接触私钥 |
| 智能代理（缓存 + 熔断 + 预取 + 指标） | ✅ | 读命令与 Pin 写命令统一走 `ProxyClient`（缓存命中率/延迟/熔断统计一致化）；`get_dashboard_stats` 的「强制刷新」路径按设计直连 API |
| 离线操作队列 + 自动重放 | ✅ | 守护进程恢复后每 15s FIFO 重放，最多重试 3 次 |
| 带宽 / 连接数配置 | ✅ | 30 点滑动窗口速率平滑 |
| 开机自启 / 系统托盘 | ✅ | |
| 长驻与自愈（Phase D2） | ✅ | **关窗→隐藏到托盘**节点后台常驻（退出走托盘 Quit）；Kubo 守护进程**崩溃自动重启**（线性退避 + 上限 5 次 + 持续健康 30s 清零预算），`config.auto_restart` 控制（默认开） |
| 节点身份（Phase D1） | ✅ | 人类可读标签 ↔ 节点密码学身份（Kubo PeerID / iroh 自证公钥），可编辑、可导出验证；见仪表盘「身份卡」 |
| 节点健康度（Phase D3） | ✅ | 仪表盘「健康度」卡：应用/节点**在线时长** + 仓库对象数/大小 + 连接数 + 累计收发字节（贡献量）+ iroh 内容数（`get_node_health`） |
| 查询参数安全 | ✅ | 所有用户输入参数经 URL 百分号编码 |
| Kubo vs iroh 基准 & 兼容性测试 | ✅ | `benchmark.rs` 已产出**真实** iroh add/cat 延迟数据（本机原生，亚毫秒级）；Kubo 侧需守护进程运行才有对比值 |
| **iroh 原生后端** | 🟡 | 真实实装（`--features iroh-backend`，iroh 1.0 + iroh-blobs）：`node_info` / `add_file` / `cat`、**持久化节点身份**（跨重启稳定）、**serving + 两节点 QUIC 互传**、**生命周期**（`shutdown` → 从磁盘自动重启，内容留存）、**keep-alive**（命名 tag 保护内容免 GC）。IPNS/Pin 在 iroh 语义下不适用（返回 `Unsupported`）；`swarm_peers` 因 iroh 无节点枚举 API，改为**会话内双向追踪对端**（fetch 登记 outbound；`ClientConnectedNotify` 事件登记 inbound；双向为 both）；UI 暂不开放切换 |

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

**iroh 原生后端（实验，Phase B）**：默认不编译 iroh（重依赖）。启用后编译 `iroh_adapter.rs` 的真实实现并可跑本机 add→cat 往返测试与真实基准：

```bash
# 真实往返 + 身份持久化 + 两节点 QUIC 互传（node_info / add_file / cat / serving）
cargo test --manifest-path src-tauri/Cargo.toml --features iroh-backend --lib real_tests -- --nocapture

# 产出第一份真实 add/cat 延迟对比数据
cargo test --manifest-path src-tauri/Cargo.toml --features iroh-backend --lib test_real_add_cat_comparison -- --nocapture
```

> **Windows 依赖提示**：iroh 1.0 的网络层经 `netwatch` 引入 `wmi`；`wmi 0.18.4` 与 `windows-core 0.62` 存在冲突，本仓库 `Cargo.lock` 已将 `wmi` 固定到 `0.18.2`。若 `cargo update` 后 iroh 构建报 `windows-core` 相关错误，执行 `cargo update -p wmi --precise 0.18.2` 复位。

**iroh 原生收发命令（Phase B-a，GUI 可直接 `invoke`）**：

| 命令 | 作用 |
|------|------|
| `iroh_add_file(file_path)` | iroh 原生添加文件，返回 BLAKE3 hash |
| `iroh_node_info()` | 节点持久身份 / 版本 |
| `iroh_share(cid)` | 为本地 blob 生成可分享 **BlobTicket** 字符串 |
| `iroh_fetch_ticket(ticket, save_path?)` | 用 ticket 跨节点收取内容，可保存到本地 |
| `iroh_register_ticket(ticket)` | 登记 provider（**不立即拉取**）；之后 `Auto` 下 cat 该 CID 本地 miss 时自动网络取回 |
| `iroh_keep(cid)` / `iroh_unkeep(cid)` | keep-alive：命名持久 tag 保护内容免 GC / 取消（对应 Kubo 的 pin）|
| `iroh_shutdown()` | 关闭 iroh 网络/serving 栈（Phase D2 生命周期）；下次使用从磁盘自动重建 |

未启用 feature 时这些命令返回「需启用 iroh-backend」的错误（stub 优雅降级）。

**双栈路由骨架（Phase C-b）**：`backend_router.rs` 在 `Backend` 缝之上按策略/内容选后端。

| 命令 | 作用 |
|------|------|
| `get_route_policy()` / `set_route_policy(policy)` | 读写策略：`KuboOnly`（默认，零回归）/ `IrohOnly` / `Auto` |
| `get_backend_route(cid)` | 查询某 CID 在当前策略下会路由到哪个后端 |

- **`Auto` 三级决策链（按内容实际所在路由，Phase C 核心）**：
  1. **来源标记**（已知事实）——add 成功后记录「CID → 产生它的后端」，持久化到 `cache_dir/cid_origins.json`；
  2. **内容发现**——无标记时**实测 iroh 本地是否真有该 blob**（`store.has`），有则走 iroh（靠实测，不靠猜测）；
  3. **前缀启发式**——兜底（`Qm.../baf...` → Kubo，其余 → iroh）。

  这把最脆弱的前缀猜测降为最后手段。iroh 侧 hash 解析已做形态预校验（64 位 hex / 52 位 base32），避免把 Kubo CID 喂给 iroh 解析器时触发 panic。
- **`cat_file` / `add_file` 已接入路由**：默认 `KuboOnly` 下行为与原来完全一致（Kubo 路径仍用实时 `api_client`，尊重运行时改地址）；切到 `Auto`/`IrohOnly` 才真正分流到 iroh。
- **双栈韧性（fallback-on-miss）**：`Auto` 下 `cat` 的后端取不到时按 `[主选, 另一个, 网络]` 三级 fallback：
  1. 本地主后端 →
  2. 本地另一后端 →
  3. **网络**：若已知该 CID 的 iroh provider（来自收过/登记过的 ticket），从远端节点跨网取回。

  任一级命中后**回填来源标记**（自愈，下次直达）；全部失败才返回主后端错误。`KuboOnly`/`IrohOnly` 是显式选择，不做跨栈 fallback。
- **写侧策略**：`add_file` 支持可选 `prefer`（`"iroh"`/`"kubo"`）——「本地/信任圈内容优先 iroh」由此显式表达；省略即按策略（`Auto` 默认落 Kubo 以保证公网可寻址），零回归。
- **前端可视化**：文件面板输入 CID 时实时显示「🔀 路由到 → Kubo/iroh」（`get_backend_route`，`Auto` 下含本地内容探测）。
- **前端已有 "iroh 原生" 标签页**：可视化「选文件 → 原生添加 → 生成分享 ticket」与「粘贴 ticket → 收取并保存」，并可切换路由策略——GUI 真正能收发。

**Phase D 里程碑自检（可信个人节点）**：`scripts/d5-selfcheck.sh` 跑可自动化的 D5 判据（构建/lint、身份稳定、自愈默认、路由韧性；`--iroh` 加核验内容完整性/生命周期/两节点互传/keep-alive）。手动/长期观察项（长跑数周、NAT 可达性、常驻内存、OS 加密）见 [PHASE_D5_CHECKLIST.md](PHASE_D5_CHECKLIST.md)。

```bash
bash scripts/d5-selfcheck.sh --iroh   # 自动化部分全 PASS 即达标
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
│   │   ├── lib.rs              # 入口：插件注册 + 55 命令注册 + 日志初始化 + 托盘
│   │   ├── main.rs            # 程序入口
│   │   ├── commands.rs        # 55 个 #[tauri::command]（所有状态变更的唯一入口）
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
│   │   ├── iroh_adapter.rs     # iroh → Backend 适配器（stub / 真实实现，由 iroh-backend feature 门控）
│   │   ├── backend_router.rs   # 双栈路由骨架（Phase C）：按策略/内容在 Kubo↔iroh 间选后端 + 三级 fallback
│   │   ├── identity.rs         # 节点身份记录（Phase D1）：人类可读标签 ↔ 节点密码学身份
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

---

## 贡献者--至WEB3领域，对于践行去中心化的伟大事业的先驱者以及对人类的自由意志致以最崇高的敬意
<a href="https://github.com/xuanxi369/IPFS-Desktop-Rust/graphs/contributors">
  <img src="https://contrib.rocks/image?repo=xuanxi369/IPFS-Desktop-Rust" />
</a>

---

## License

MIT

## 相关资源

- [Tauri 文档](https://tauri.app/)
- [IPFS Desktop（原版）](https://github.com/ipfs/ipfs-desktop)
- [Kubo (go-ipfs)](https://github.com/ipfs/kubo)
- [iroh](https://github.com/n0-computer/iroh)
