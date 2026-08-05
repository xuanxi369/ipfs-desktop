# prod 分支变更清单

本文记录本次发布到 `prod` 分支的全部文件级改动。

## 根目录与工程配置

| 文件 | 改动 |
| --- | --- |
| `.github/workflows/build.yml` | 前端检查加入 Vitest；新增启用 `iroh-backend` 的 Linux 检查与测试任务；Rustfmt 改为只检查、不在 CI 中修改源码。 |
| `README.md` | 按当前项目实际功能重写使用说明，补充架构、环境、启动、前端检查、Kubo 故障排查、数据目录、安全和文档入口。 |
| `COMMANDS.md` | 新增 Tauri 命令分类索引，覆盖守护进程、文件、Pin、IPNS、MFS、双后端、iroh 和诊断命令。 |
| `package.json` | 增加 Vitest 与 Testing Library 测试工具；移除未使用的 opener、shell、fs 前端插件依赖；增加 `npm test`。 |
| `package-lock.json` | 同步 npm 依赖树、测试依赖及已移除插件的锁定信息。 |
| `vitest.config.ts` | 新增 React/jsdom 测试环境配置。 |
| `CHANGELOG_IMPROVEMENTS.md` | 从根目录移除，内容迁移到 `docs/stages/CHANGELOG_IMPROVEMENTS.md`。 |
| `COMPLETION_REPORT.md` | 从根目录移除，内容迁移到 `docs/stages/COMPLETION_REPORT.md`。 |
| `PHASE_D4_ENCRYPTION_SPIKE.md` | 从根目录移除，内容迁移到 `docs/stages/PHASE_D4_ENCRYPTION_SPIKE.md`。 |
| `PHASE_D5_CHECKLIST.md` | 从根目录移除，内容迁移到 `docs/stages/PHASE_D5_CHECKLIST.md`。 |
| `PHASE_D_PLAN.md` | 从根目录移除，内容迁移到 `docs/stages/PHASE_D_PLAN.md`。 |
| `项目路线.md` | 从根目录移除，内容整理到 `docs/PROJECT_ROADMAP.md`。 |

## 文档目录

| 文件 | 改动 |
| --- | --- |
| `docs/README.md` | 新增文档索引，区分正式使用说明、命令索引、路线图和阶段资料。 |
| `docs/PROJECT_ROADMAP.md` | 整理项目长期路线，说明 Kubo 兼容锚点、iroh 原生快车道和双栈迁移策略。 |
| `docs/stages/CHANGELOG_IMPROVEMENTS.md` | 保存原改进日志并归档到阶段文档目录。 |
| `docs/stages/COMPLETION_REPORT.md` | 保存原完成报告并归档到阶段文档目录。 |
| `docs/stages/PHASE_D4_ENCRYPTION_SPIKE.md` | 保存 Phase D4 加密研究记录。 |
| `docs/stages/PHASE_D5_CHECKLIST.md` | 保存 Phase D5 验收清单。 |
| `docs/stages/PHASE_D_PLAN.md` | 保存 Phase D 阶段计划。 |
| `docs/PROD_CHANGELOG.md` | 新增本次 prod 发布的逐文件变更记录。 |

## Tauri 与 Rust 后端

| 文件 | 改动 |
| --- | --- |
| `src-tauri/Cargo.toml` | 移除未使用的 opener、shell、fs 插件；加入 single-instance 插件。 |
| `src-tauri/Cargo.lock` | 同步 Rust 依赖锁定结果，包括 single-instance 和插件依赖调整。 |
| `src-tauri/capabilities/default.json` | 收紧权限，仅允许文件打开和保存对话框，不再暴露未使用的 shell、fs 和 opener 默认权限。 |
| `src-tauri/tauri.conf.json` | 增加内容安全策略，限制连接、图片、样式和脚本来源。 |
| `src-tauri/src/atomic_file.rs` | 新增原子写入工具，通过临时文件、同步和重命名降低配置文件损坏风险。 |
| `src-tauri/src/backend_router.rs` | CID 来源和 provider 映射改用原子写入持久化。 |
| `src-tauri/src/commands.rs` | 支持连接已存在的 Kubo；改进下载总长度与 iroh 文件导出；新增 Peer 地理命令；结构化映射 iroh 错误；加强输出路径校验；将 MFS 命令拆出独立模块。 |
| `src-tauri/src/commands_mfs.rs` | 新增独立 MFS 命令模块，对路径、根目录删除和移动操作进行安全校验。 |
| `src-tauri/src/config.rs` | 配置改为原子保存；严格验证 API/Gateway URL；远程 API 强制 HTTPS。 |
| `src-tauri/src/daemon/api_client.rs` | Kubo 只读 RPC 改用 POST；兼容多种 `pin/ls` 返回格式；增加文件大小查询并整理 MFS 请求。 |
| `src-tauri/src/daemon/binary.rs` | 改进 Kubo 可执行文件发现、进程输出和错误诊断相关处理。 |
| `src-tauri/src/daemon/kubo_hashes.rs` | 整理各平台 Kubo 哈希表格式，保持平台校验数据可读。 |
| `src-tauri/src/error.rs` | 增加带类型和消息的结构化 Backend 错误。 |
| `src-tauri/src/identity.rs` | 节点身份记录改用原子写入，避免写入中断造成损坏。 |
| `src-tauri/src/iroh_adapter.rs` | 增加 ticket 直接下载到文件和本地 blob 直接导出，避免完整内容常驻内存；stub 模式返回明确不支持错误。 |
| `src-tauri/src/lib.rs` | 注册原子写入、MFS、安全路径和 Peer 地理模块；启用单实例；注册 Peer 地图和拆分后的 MFS 命令。 |
| `src-tauri/src/path_security.rs` | 新增下载输出路径、符号链接、应用数据目录和 MFS 路径穿越防护及测试。 |
| `src-tauri/src/peer_geo.rs` | 新增公网 Peer 地址提取、过滤、限流 GeoIP 查询、国家统计和近似坐标报告。 |
| `src-tauri/src/state.rs` | Kubo 启动改为轮询 RPC 就绪状态；保存进程所有权；超时返回明确错误；支持安全连接外部已运行的 Kubo。 |

## React 前端

| 文件 | 改动 |
| --- | --- |
| `src/App.tsx` | 接入高级工具；补充内容记录删除、iroh keep/unkeep/shutdown/provider 登记；二进制预览改为十六进制；开放离线输入；补齐界面翻译。 |
| `src/App.css` | 增加 Peer 地图、点位动画、提示框、区域列表、基准历史、内容表格和路由说明样式；清除旧地图瓦片遗留规则。 |
| `src/styles.css` | 为禁用的按钮和表单控件增加统一视觉与鼠标状态。 |
| `src/AdvancedTools.tsx` | 新增 API/Gateway 配置、MFS 操作和 Kubo 二进制校验高级工具页面。 |
| `src/AdvancedTools.test.tsx` | 新增高级工具页面渲染、配置保存和离线状态测试。 |
| `src/Dashboard.tsx` | 接入 Peer 连接地图；保存并展示本地基准测试历史。 |
| `src/Files.tsx` | 增加内容索引删除；允许输入 CID 和选择文件；补齐翻译；让路由层决定后端是否可用。 |
| `src/IpnsManager.tsx` | 输入控件不再因前端状态误锁定；刷新和操作由后端返回真实错误；补齐页面说明翻译。 |
| `src/IrohNative.tsx` | 增加 keep、unkeep、只登记 ticket、关闭 iroh 和路由策略解释。 |
| `src/PeerMap.tsx` | 新增本地 SVG 世界地图、Peer 点位、地区统计、刷新、离线探测和详情提示。 |
| `src/PinManager.tsx` | 修复输入和操作按钮被前端状态错误禁用；保留加载态防重复刷新；补齐翻译。 |
| `src/WebUI.tsx` | 补齐说明翻译；调整 iframe 限制以允许 Kubo WebUI 正常加载。 |
| `src/benchmarkHistory.ts` | 新增基准结果的 localStorage 持久化、数量限制和损坏数据恢复。 |
| `src/benchmarkHistory.test.ts` | 测试基准记录保存、排序和读取。 |
| `src/locales/en.json` | 增加高级工具、状态、错误、文件、Pin、IPNS、WebUI 和 Peer 地图英文翻译。 |
| `src/locales/zh.json` | 增加高级工具、状态、错误、文件、Pin、IPNS、WebUI 和 Peer 地图中文翻译。 |
| `src/types.ts` | 增加结构化 Backend 错误解析和 `advanced` 标签类型。 |
| `src/types.test.ts` | 增加字节格式化和结构化错误展示测试。 |

## 验证范围

- `npm run typecheck`
- `npm test`
- `npm run build`
- `git diff --check`

本次按项目开发约束未在本地运行 Cargo 命令；Rust 与 iroh 检查由 CI 工作流执行。
