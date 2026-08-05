# IPFS Desktop Rust

一个基于 Tauri 2、Rust、React 和 TypeScript 的 IPFS 桌面客户端。它提供桌面化的 Kubo 管理界面，并将文件、Pin、IPNS、节点状态和网络连接集中到一个应用中。

## 项目定位

本项目的节点核心是 Go 版 Kubo（`ipfs` 守护进程），Rust/Tauri 负责桌面窗口、进程生命周期、HTTP API 访问、缓存、离线队列和系统托盘；React 前端负责交互界面。

项目还保留了 iroh 原生后端和双后端路由的扩展接口。默认使用 Kubo，以保证与现有 IPFS 网络和 CID 兼容；iroh 相关能力属于实验性功能，不应视为 Kubo 的完全替代品。

## 当前功能

- 守护进程启动、停止、重启和状态监控
- 仪表盘：节点状态、Peer 数量、带宽、Bitswap、仓库信息和节点健康度
- Peer 连接地图：展示当前可定位公网 Peer 的大致区域，不代表全球 IPFS 节点总量
- 文件上传、CID 预览、流式下载和下载进度显示
- Pin 列表、添加和移除 Pin
- IPNS 发布、解析和 Kubo 密钥标签管理
- 内置 IPFS WebUI
- 缓存、离线操作队列、自动重放和后台健康监控
- 系统托盘、开机自启和 Kubo 崩溃自动重启
- 高级工具：API/Gateway 地址配置、MFS 调试、二进制信息查看和路由策略
- 中英文界面切换
- 可选 iroh 原生文件收发、BlobTicket、keep/unkeep 和双后端路由实验功能

## 环境要求

- Node.js 18 或更高版本
- npm
- Rust stable toolchain（用于 Tauri 开发和打包）
- 已安装的 Kubo（命令名通常为 `ipfs`）
- Tauri 2 的系统依赖
  - macOS：Xcode Command Line Tools
  - Windows：Microsoft C++ Build Tools 和 WebView2
  - Linux：WebKitGTK、编译器和窗口系统开发包，详见 [Tauri prerequisites](https://tauri.app/start/prerequisites/)

应用查找 Kubo 的顺序如下：

1. `IPFS_GO_EXEC` 环境变量指定的可执行文件
2. 系统 `PATH` 中的 `ipfs`（Windows 为 `ipfs.exe`）
3. 应用目录、`bin/` 或 `resources/` 中的内置二进制

首次使用前初始化仓库：

```bash
ipfs init
```

## 快速开始

```bash
npm install
npm run tauri dev
```

开发窗口启动后，进入控制面板并点击启动守护进程。Kubo RPC 默认地址为 `http://127.0.0.1:5001`，Gateway 默认地址为 `http://127.0.0.1:8080`。

仅调试前端界面时可以运行：

```bash
npm run dev
```

这种模式不会启动 Rust/Tauri 后端，调用守护进程的按钮无法正常工作。

## 前端检查

项目提供以下不依赖 Cargo 的前端命令：

```bash
npm run typecheck
npm test
npm run build
```

`npm run build` 会生成可重新创建的 `dist/` 目录。该目录不应提交到版本库。

## Kubo 启动故障排查

如果界面提示守护进程启动失败，先确认没有旧的 `ipfs`/`kubo` 进程占用端口。

macOS/Linux：

```bash
pgrep -af '(^|/)(ipfs|kubo)( |$)'
pkill -TERM -f '(^|/)(ipfs|kubo)( |$)'
sleep 2
pkill -KILL -f '(^|/)(ipfs|kubo)( |$)'
```

Windows PowerShell：

```powershell
Get-Process ipfs,kubo -ErrorAction SilentlyContinue
Stop-Process -Name ipfs,kubo -Force -ErrorAction SilentlyContinue
```

确认 RPC 端口是否被占用：

```bash
lsof -nP -iTCP:5001 -sTCP:LISTEN
```

也可以在高级工具中检查 API/Gateway 地址，或使用 `IPFS_GO_EXEC` 指向明确的 Kubo 二进制：

```bash
IPFS_GO_EXEC=/path/to/ipfs npm run tauri dev
```

应用日志位置：

- macOS：`~/Library/Application Support/ipfs-desktop-rust/logs/`
- Linux：`~/.local/share/ipfs-desktop-rust/logs/`
- Windows：`%LOCALAPPDATA%\\ipfs-desktop-rust\\logs\\`

## 数据位置

应用数据保存在系统应用数据目录下：

| 数据 | 文件 |
| --- | --- |
| 配置 | `config.json` |
| SQLite 缓存 | `cache.db` |
| 离线队列 | `offline_queue.db` |
| IPNS 公开标签记录 | `keys/*.json` |
| 日志 | `logs/app.log` |

IPNS 私钥由 Kubo 密钥库管理，本应用不生成、不保存，也不通过 IPC 传输私钥。

## 项目结构

```text
src/                       React 前端、页面组件、样式和国际化
src-tauri/src/             Rust/Tauri 后端
src-tauri/src/commands.rs  前端可调用的 Tauri 命令
src-tauri/src/daemon/      Kubo 二进制、进程控制和 RPC 客户端
src-tauri/src/peer_geo.rs  Peer 公网地址解析和区域定位
src-tauri/src/backend_router.rs  Kubo/iroh 路由策略
docs/                      路线图、阶段记录和验收文档
COMMANDS.md                Tauri 命令索引
```

## 安全与隐私

- Peer 地图只处理可获得的公网地址，并显示大致区域；内网、Relay、DNS 和匿名地址会被过滤或标记为未知。
- 地理定位结果来自 IP 地址推断，存在误差，不代表节点的精确位置。
- Kubo API 默认只连接本机地址。修改 API 地址前请确认网络访问控制和防火墙策略。
- 不要把配置目录、日志或 Kubo 密钥库提交到公共仓库。

## 相关文档

- [Tauri 命令索引](COMMANDS.md)
- [项目文档索引](docs/README.md)
- [项目路线图](docs/PROJECT_ROADMAP.md)

---

## Contributors - - 向贡献者与‘去中心化’的先驱者致敬，向人类的自由意志致敬

<a href="https://github.com/xuanxi369/IPFS-Desktop-Rust/graphs/contributors">
  <img src="https://contrib.rocks/image?repo=xuanxi369/IPFS-Desktop-Rust" />
</a>

---

## 许可证

仓库当前未声明统一许可证。对外发布前请补充 LICENSE 文件，并确认 Kubo、Tauri 及其他依赖的许可证要求。
