# IPFS Desktop Rust

[简体中文](README.md) | [English](README_EN.md)

一个基于 Tauri 2、Rust、React 和 TypeScript 的桌面内容节点。项目采用双线演进策略：以成熟的 Kubo 生态承担 IPFS、IPNS、MFS 和 Gateway 兼容能力，同时让 iroh 负责本地新增内容与原生点对点传输，逐步把 Kubo 从默认运行时降为按需兼容桥。

> 当前版本：`0.2.1`。项目处于开发与验证阶段，尚未宣称达到生产级长期在线节点标准。

## 为什么是双后端

Kubo 提供成熟的 IPFS 互操作性，iroh 提供 Rust 原生、面向内容的快速传输。项目没有要求用户在两个后端之间做技术选择，而是在统一的 `Backend` 能力接口与 `ContentRef` 之上提供三种使用模式：

| 使用模式 | 新增内容 | 读取与兼容行为 | 适用场景 |
| --- | --- | --- | --- |
| `Compatible`（默认） | 优先写入 iroh | 根据内容来源、本地探测和映射自动路由；IPFS/IPNS/Gateway 操作按需使用 Kubo | 日常使用与渐进迁移 |
| `LocalFirst` | 仅使用 iroh 原生路径 | 不主动依赖 Kubo 兼容能力 | 本地内容和可信节点直传 |
| `Mirrored` | 同时写入 iroh 与 Kubo | 写入后读取两份内容并进行字节与 SHA-256 校验 | 迁移验证和高兼容需求 |

`ContentMapping` 持久化 Kubo CID、iroh hash、大小和 SHA-256 的对应关系。路由不再根据 CID 前缀猜测后端：IPFS 引用必须通过 CID crate 进行语义解析，iroh 引用必须显式标识或来自可信映射与实际内容探测。

## 当前能力

- Kubo 生命周期管理：首次仓库初始化、启动、停止、重启、状态检测和崩溃恢复。
- iroh 原生内容：持久节点身份、文件 add/cat、BlobTicket 分享与接收、keep/unkeep、关闭后重建。
- 内容工作区：文件选择、流式上传、下载进度、CID 预览、内容索引和后端来源显示。
- IPFS 兼容能力：Pin、IPNS 密钥与发布/解析、MFS、Web UI 和 Gateway。
- 双后端路由：Auto fallback、来源/provider 持久化、Mirror 双写和字节一致性验证。
- 节点观察：Peer、带宽、仓库、Bitswap、健康度、迁移进度、兼容状态和基准历史。
- 桌面体验：系统托盘、开机启动、单实例、中英文界面、亮色/暗色主题。
- 可靠性：SQLite 缓存、离线队列、原子 JSON 写入、每日无丢弃日志和 Kubo 重复告警降噪。

## 架构

```text
React UI
  └─ hooks: daemon / content / IPNS / iroh / theme
       └─ Tauri commands（按领域拆分）
            ├─ BackendRouter
            │    ├─ IrohBackend（默认本地内容路径）
            │    └─ KuboBackend（IPFS 兼容桥）
            ├─ cache / offline queue / content index
            └─ daemon controller / monitoring / tray
```

关键设计文件：

- `src-tauri/src/backend_trait.rs`：后端能力接口、`ContentRef` 和统一错误模型。
- `src-tauri/src/backend_router.rs`：使用模式、路由、fallback、映射和 Mirror 校验。
- `src-tauri/src/iroh_adapter.rs`：真实 iroh 实现及无 feature 的 stub 兼容实现。
- `src-tauri/src/kubo_adapter.rs`：Kubo HTTP API 后端适配。
- `src-tauri/src/commands/`：daemon、config、content、IPNS、iroh、identity 和 monitoring 命令。
- `src/hooks/`：前端业务状态与 Tauri 事件处理。

## 构建变体

默认 feature 已包含真实 `iroh-backend`：

| 能力 | 默认构建 | `--no-default-features` |
| --- | --- | --- |
| iroh 原生 add/cat、ticket、keep | 可用 | stub，操作返回不支持 |
| Kubo、Pin、IPNS、MFS、Gateway | 可用 | 可用 |
| `Compatible` / `LocalFirst` / `Mirrored` | 可用 | 仅 Kubo 兼容路径适合实际使用 |
| 推荐用途 | 开发、双栈验证、日常试用 | Kubo-only 排障与兼容测试 |

## 环境要求

- Windows 10/11、macOS 或 Linux。
- Node.js 18 或更高版本和 npm。
- Rust stable 工具链。
- Tauri 对应平台的系统依赖。
- Kubo：可由 Windows 设置脚本下载到 Tauri resources，也可使用 `IPFS_GO_EXEC` 或系统 `PATH` 中的现有二进制。

## 快速开始

安装前端依赖：

```bash
npm install
```

Windows 开发环境可下载并校验官方 Kubo sidecar：

```powershell
npm run setup:kubo
```

启动桌面应用：

```bash
npm run tauri dev
```

只运行前端：

```bash
npm run dev
```

构建生产版本：

```bash
npm run tauri build
```

Kubo-only 兼容构建：

```bash
npm run tauri dev -- --no-default-features
```

## 验证

前端检查：

```bash
npm run typecheck
npm test
npm run build
```

Rust 默认测试（默认已包含真实 iroh）：

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
cargo test --manifest-path src-tauri/Cargo.toml --lib
```

显式 iroh feature 与双节点集成测试：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib --features iroh-backend
cargo test --manifest-path src-tauri/Cargo.toml --features iroh-backend --test iroh_two_node -- --nocapture
```

无 iroh feature 的 stub 兼容测试：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib --no-default-features
```

完整发布检查见 [`docs/RELEASE_CHECKLIST.md`](docs/RELEASE_CHECKLIST.md)。

## 数据与日志

应用数据默认位于：

- Windows：`%LOCALAPPDATA%\ipfs-desktop-rust\`
- macOS：`~/Library/Application Support/ipfs-desktop-rust/`
- Linux：`~/.local/share/ipfs-desktop-rust/`

主要内容：

| 路径 | 用途 |
| --- | --- |
| `config.json` | 应用配置与使用模式 |
| `identity.json` | 应用节点标签和创建时间 |
| `content_index.db` | 本地内容索引 |
| `cache.db` | Dashboard/API 缓存 |
| `offline_queue.db` | 离线操作队列 |
| `backend-origins.json` | 内容来源记录 |
| `content-mappings.json` | Kubo CID 与 iroh hash 映射 |
| `iroh-providers.json` | 已登记的 iroh provider ticket |
| `iroh-data/` | iroh 身份与 blob 数据 |
| `logs/app.log.YYYY-MM-DD` | 每日滚动应用、Kubo 和 iroh 日志 |

日志采用无丢弃后台写入；文件会在应用运行期间持续增长。`RUST_LOG` 可调整日志级别。

## 安全边界

- Kubo API 和 Gateway 默认仅允许本机地址。
- 远程 API 必须显式解锁，只接受 HTTPS，并拒绝解析到 loopback、私网、链路本地、组播和其他特殊地址的目标。
- HTTP 客户端不使用系统/企业代理访问 Kubo 控制接口，并固定已验证的 DNS 解析结果以降低 DNS 重绑定风险。
- 可通过 `IPFS_API_AUTHORIZATION` 环境变量为远程反向代理提供认证头，凭据不会写入 `config.json`。
- CID 使用成熟的 `cid` crate 严格解析；MFS 路径和下载目标具有独立的边界校验。
- Windows Kubo sidecar 下载脚本校验官方 SHA-512；应用还支持固定本地二进制 SHA-256。
- IPNS 私钥由 Kubo 密钥库管理，不通过 Tauri IPC 返回或写入应用 JSON。

不要将应用数据目录、Kubo 仓库、日志、ticket、认证环境变量或私钥提交到公共仓库。

## 已知限制

- iroh 与 Kubo 是不同协议栈；iroh 内容不会天然成为公共 IPFS CID，互操作依赖显式映射或 Mirrored 模式。
- iroh 不原生提供 IPNS、MFS、Bitswap 或 Gateway，这些能力仍由 Kubo 兼容桥承担。
- mDNS 在部分 Windows 虚拟网卡上可能无法设置组播接口；这通常只影响相应网卡的局域网自动发现，不影响公网 DHT 或 iroh relay。
- 跨机器、跨 NAT、休眠唤醒以及数天级 soak test 仍需持续验证。
- 远程 Kubo API 属于高级能力，即使已有 SSRF 防护，也应只连接受信任服务。

## 项目结构

```text
src/                         React UI、hooks、i18n 与前端测试
src-tauri/src/               Rust 核心、路由、后端和 Tauri commands
src-tauri/src/commands/      按业务领域拆分的 IPC 命令
src-tauri/tests/             Rust 集成测试
src-tauri/resources/         Kubo sidecar 说明（实际二进制不入库）
scripts/                     开发与依赖准备脚本
docs/                        路线图、阶段记录、编码与发布检查
.github/workflows/           CI 构建与双 feature 测试矩阵
```

## 文档

- [项目路线图](docs/PROJECT_ROADMAP.md)
- [文档索引](docs/README.md)
- [编码约定](docs/ENCODING.md)
- [发布检查清单](docs/RELEASE_CHECKLIST.md)
- [贡献指南](CONTRIBUTING.md)
- [English README](README_EN.md)

路线图中的长期愿景不等于当前版本承诺；以本 README 的“当前能力”和实际测试结果为准。

## 许可证

项目采用 `MIT OR Apache-2.0` 双重许可：

- [MIT License](LICENSE-MIT)
- [Apache License 2.0](LICENSE-APACHE)

## 致谢  -  向贡献者与‘去中心化’的先驱者致敬，向人类的自由意志致敬

<a href="https://github.com/xuanxi369/IPFS-Desktop-Rust/graphs/contributors">
  <img src="https://contrib.rocks/image?repo=xuanxi369/IPFS-Desktop-Rust" />
</a>

感谢 IPFS、Kubo、iroh、Tauri、Rust 和开源社区的贡献者。本项目希望在保留 IPFS 生态互操作性的同时，探索一个更轻量、可验证、可长期演进的个人内容节点。
