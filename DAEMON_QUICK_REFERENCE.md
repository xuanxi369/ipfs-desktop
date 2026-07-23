# Daemon 模块快速参考

## 📂 项目结构

```
src-tauri/src/
├── daemon/
│   ├── mod.rs           (260 bytes)   - 模块导出
│   ├── binary.rs        (4.9 KB)     - 二进制查找
│   ├── controller.rs    (9.6 KB)     - 进程控制
│   └── api_client.rs    (9.3 KB)     - HTTP API 客户端
├── config.rs            (5.3 KB)     - 配置持久化
├── commands.rs          (7.3 KB)     - Tauri 命令
├── state.rs             (2.3 KB)     - 应用状态
├── types.rs             (1.5 KB)     - 类型定义
├── lib.rs               (2.4 KB)     - 库入口
└── main.rs              (192 bytes)  - 主程序入口

总计: ~1419 行代码
```

## 🔑 关键组件

### 1. BinaryFinder (binary.rs)
```rust
// 查找 IPFS 二进制
let binary_path = BinaryFinder::find();

// 获取版本
let version = BinaryFinder::get_version(&path)?;
```

### 2. DaemonController (controller.rs)
```rust
// 创建控制器
let controller = DaemonController::new(binary_path, repo_path);

// 启动
controller.start(vec!["--migrate=true".to_string()]).await?;

// 停止
controller.stop().await?;

// 重启
controller.restart(flags).await?;

// 检查状态
let is_running = controller.is_running().await;
let status = controller.get_status().await;
let pid = controller.get_pid().await;
```

### 3. IpfsApiClient (api_client.rs)
```rust
// 创建客户端
let client = IpfsApiClient::new("http://127.0.0.1:5001".to_string());

// 获取节点 ID
let node_id = client.id().await?;

// 获取版本
let version = client.version().await?;

// 仓库统计
let stats = client.repo_stat().await?;

// 对等节点
let peers = client.swarm_peers().await?;

// 垃圾回收
client.repo_gc().await?;

// 关闭守护进程
client.shutdown().await?;

// 检查可达性
let reachable = client.is_reachable().await;
```

### 4. AppConfig (config.rs)
```rust
// 加载配置
let config = AppConfig::load()?;

// 保存配置
config.save()?;

// 验证配置
config.validate()?;

// 获取 IPFS 路径
let path = config.get_ipfs_path();

// 获取配置文件路径
let config_path = AppConfig::config_path();
```

## 🎯 Tauri 命令

### 前端调用示例

```typescript
import { invoke } from '@tauri-apps/api/core';

// 启动守护进程
await invoke('start_daemon');

// 停止守护进程
await invoke('stop_daemon');

// 重启守护进程
await invoke('restart_daemon');

// 获取状态
const status = await invoke('get_daemon_status');

// 获取配置
const config = await invoke('get_config');

// 更新配置
await invoke('update_config', { newConfig: {...} });

// 获取节点 ID
const nodeId = await invoke('get_node_id');
```

### 监听事件

```typescript
import { listen } from '@tauri-apps/api/event';

// 监听状态变化
const unlisten = await listen('daemon-status-changed', (event) => {
  console.log('Daemon status:', event.payload);
});
```

## 🔄 状态类型

```rust
pub enum DaemonStatus {
    Stopped,
    Starting,
    Running { 
        pid: u32, 
        peer_id: String,
        api_addr: String,
    },
    Stopping,
    Failed { 
        error: String 
    },
}
```

## ⚙️ 配置结构

```rust
pub struct AppConfig {
    pub ipfs_path: Option<PathBuf>,    // None = 使用默认 ~/.ipfs
    pub api_addr: String,              // http://127.0.0.1:5001
    pub gateway_addr: String,          // http://127.0.0.1:8080
    pub daemon_flags: Vec<String>,     // ["--migrate=true", ...]
    pub auto_launch: bool,             // 开机自启
    pub auto_gc: bool,                 // 自动垃圾回收
}
```

## 🚀 启动流程

```
1. start_daemon 被调用
   ↓
2. 检查当前状态 (必须是 Stopped 或 Failed)
   ↓
3. 设置状态为 Starting
   ↓
4. 发送 "daemon-status-changed" 事件
   ↓
5. BinaryFinder::find() 查找二进制
   ↓
6. 创建 DaemonController
   ↓
7. controller.start(flags) 启动进程
   ↓
8. 使用 IpfsApiClient 获取节点信息
   ↓
9. 设置状态为 Running
   ↓
10. 发送 "daemon-status-changed" 事件
```

## 🛑 停止流程

```
1. stop_daemon 被调用
   ↓
2. 检查当前状态 (如果已停止则直接返回)
   ↓
3. 设置状态为 Stopping
   ↓
4. 发送 "daemon-status-changed" 事件
   ↓
5. controller.stop()
   ↓
   ├─ Unix: 发送 SIGTERM
   │         等待 5 秒
   │         如果未退出，发送 SIGKILL
   │
   └─ Windows: 直接 kill
   ↓
6. 设置状态为 Stopped
   ↓
7. 清理 controller
   ↓
8. 发送 "daemon-status-changed" 事件
```

## 🔍 二进制查找顺序

```
1. 环境变量 IPFS_GO_EXEC
   ↓ (如果未设置)
2. 系统 PATH 中的 ipfs
   ↓ (如果未找到)
3. 应用程序目录中的内置二进制
   - ./ipfs (或 ipfs.exe)
   - ./bin/ipfs
   - ./resources/ipfs
   ↓ (如果都未找到)
4. 返回 None
```

## 📝 日志级别

设置环境变量来控制日志级别：

```bash
# 所有日志
export RUST_LOG=info

# 只看 daemon 模块
export RUST_LOG=ipfs_desktop_rust_lib::daemon=debug

# 多个模块
export RUST_LOG=ipfs_desktop_rust_lib::daemon=debug,ipfs_desktop_rust_lib::commands=info
```

## 🧪 测试命令

```bash
# 检查语法
cd src-tauri
cargo check

# 运行测试
cargo test

# 构建
cargo build

# 运行开发模式
cargo tauri dev
```

## 🐛 调试技巧

### 1. 查看日志文件

日志位置：
- macOS: `~/Library/Application Support/ipfs-desktop-rust/logs/app.log`
- Linux: `~/.local/share/ipfs-desktop-rust/logs/app.log`
- Windows: `%LOCALAPPDATA%\ipfs-desktop-rust\logs\app.log`

### 2. 手动测试 IPFS 二进制

```bash
# 查找二进制
which ipfs

# 测试版本
ipfs version

# 测试启动
IPFS_PATH=~/.ipfs ipfs daemon
```

### 3. 测试 API 连接

```bash
# 获取节点 ID
curl -X POST http://127.0.0.1:5001/api/v0/id

# 获取版本
curl -X POST http://127.0.0.1:5001/api/v0/version
```

## ⚠️ 常见问题

### Q: 找不到 IPFS 二进制
A: 安装 Kubo 或设置 `IPFS_GO_EXEC` 环境变量

### Q: 端口已被占用
A: 检查是否有其他 IPFS 实例在运行，或修改配置中的 API 地址

### Q: 守护进程启动失败
A: 查看日志文件，检查 IPFS 仓库是否需要迁移

### Q: API 调用超时
A: 确认守护进程正在运行，检查 API 地址是否正确

## 🎓 更多资源

- [IPFS 文档](https://docs.ipfs.tech/)
- [Kubo API 文档](https://docs.ipfs.tech/reference/kubo/rpc/)
- [Tauri 文档](https://tauri.app/v1/guides/)
- [Rust 异步编程](https://rust-lang.github.io/async-book/)

---

**最后更新**: 2025-07-23
**版本**: 1.0.0
**总代码行数**: 1419 行
