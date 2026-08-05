# Phase D 执行路径：从「能存取的节点」到「可信个人节点」

> 本文承接 [项目路线.md](../PROJECT_ROADMAP.md) 的 Phase D，基于对**当前真实代码**的就绪度评估，
> 把 D 拆成依赖有序、每步可发布的执行路径。
>
> 核心纪律：**做在 `Backend` 抽象缝上（两后端通吃）、加法式（默认零回归）、每步可停可交付**。

---

## 0. 就绪度诊断（来自逐文件分析）

| 支柱 | 就绪度 | 依据（代码事实） |
|------|--------|------------------|
| **D1 去中心化身份** | 🟢 最就绪 | iroh 已有持久化 Ed25519 密钥（`node.secret`）→ 跨重启稳定 node_id；密码学根已就位，只差「可读命名 + 可验证展示」 |
| **D2 长驻/自愈** | 🟠 有硬缺口 | 关窗即退出（无 `on_window_event` 拦截）；iroh 节点惰性 `OnceCell`、`shutdown()` 空操作、无健康监控/重启 |
| **D3 可观测性** | 🟡 半就绪 | Kubo 仪表盘已有；iroh 侧零指标；`iroh-metrics` 已在依赖树可接 |
| **D4 加密存储** | 🔴 未动 + 设计张力 | `FsStore` 明文；加密与内容寻址天然冲突，需先决策 |

**战略岔路**：Phase D 的「可信节点」不强依赖 iroh——Kubo 锚点已具备持久 PeerID + 受管守护进程 +
开机自启。因此 D 的能力应做在 `Backend` 缝这层，对 Kubo 与 iroh 都成立，不赌 iroh 何时脱离 feature 门。

---

## 1. 排序逻辑

```
D1 身份 ──┐  (最就绪，象征「先可信」，低风险先立信心)
          ├──▶ D3 可观测性 ──┐
D2 长驻 ──┘  (硬地基，是 D3 前置)   ├──▶ D5 里程碑验证
D4 加密存储(设计先行，并行推进)──┘
```

- **D1 先行**：密码学根已就位，只补 UX，低风险、立竿见影。
- **D2 是真正门槛**：「可长期在线」不成立则后面皆空谈。
- **D3 依赖 D2**：无长驻则「在线时长/服务字节」无从谈起。
- **D4 与主线解耦**：设计张力大，先出 spike，不阻塞 D1–D3。

---

## 2. Stage D1 — 身份层（可命名 · 可验证 · 可展示）

**目标**：节点拥有稳定、人类可读、可验证的身份（`My Node ↔ PeerID / iroh NodeId`）。

**步骤**
1. 身份记录模块（节点无关）：`{ label, created_at }` 持久化；node_id 由后端 `node_info` 实时取。
2. 命令：`get_node_identity()` / `set_node_label(label)` / `export_identity()`（导出自证公钥 + 标签）。
3. 前端：仪表盘「身份卡」——标签（可编辑）+ 双后端 ID（可复制）。

**涉及**：`identity.rs`、`commands.rs`、`state.rs`、`App.tsx`
**退出判据**：UI 显示可编辑标签 ↔ 稳定 ID，重启不变，可导出验证。
**风险**：低。**可发布**：✅

---

## 3. Stage D2 — 长驻与自愈（Phase D 的地基）

**目标**：应用从「关窗即死」升级为「关窗仍在、崩了自愈」的受管节点服务。

**步骤**（可分别交付）
1. ✅ **关窗→托盘常驻**：`on_window_event` 拦截 CloseRequested → `hide()` + `prevent_close()`，节点后台常驻；退出走托盘 Quit。（默认构建）
2. ✅ **Kubo 崩溃自愈**：健康监控探测意外死亡 → 自动重启，带**线性退避 + 上限（5 次）+ 持续健康 30s 后清零预算**；`config.auto_restart` 控制（默认 true，旧配置 serde 默认兼容）。（默认构建）
3. ✅ **iroh 节点生命周期**：`shutdown()` 真实关闭 Router（连带关 Endpoint）并清空 net+store 槽 → 下次使用**从磁盘自动重建（重启）**，身份跨重启持久、内容留存；`store`/`net` 由 `OnceCell` 改为 `RwLock<Option>` 以支持重置。命令 `iroh_shutdown`。（feature 构建）
4. ✅ **内容 keep-alive**：`keep(cid)` 设置命名持久 tag（`keep/<hash>`）保护内容免 GC，`unkeep` 移除；命令 `iroh_keep` / `iroh_unkeep`。（feature 构建）

**已交付**：`lib.rs`（on_window_event）、`state.rs`（`start_process` 抽取 + 自愈健康监控）、`config.rs`（`auto_restart`）、`commands.rs`（`start_daemon` 瘦身）
**退出判据（已达成）**：关窗后应用在托盘常驻；Kubo 崩溃后自动重启且防崩溃循环。
**风险**：中（已解决递归 async 的 Send/定尺寸问题——`start_process` 与监控解耦）。**可发布**：✅（默认节点「可长期在线 + 自愈」里程碑）

---

## 4. Stage D3 — 可观测性（「我的节点健康度」）

**目标**：仪表盘从「Kubo 统计」升级为「节点健康度」，两后端统一。

**步骤**
1. 统一 `NodeHealth`：在线时长、服务/接收字节、内容数、连接数、贡献量。
2. 数据源：iroh 接 `iroh-metrics`；Kubo 复用现有 stats；经缝聚合。
3. 前端：健康面板（在线时长计时、服务量、内容计数）。

**已交付**：`state.rs`（`app_started_at` / `daemon_started_at`）、`commands.rs`（`get_node_health` + `NodeHealth`）、`iroh_adapter.rs`（`content_count`）、`App.tsx`（仪表盘「节点健康度」卡）
**退出判据（已达成）**：面板显示应用/节点在线时长 + 仓库对象数/大小 + 连接数 + 收发字节（贡献量）+ iroh 内容数。
**风险**：低–中。**可发布**：✅

---

## 5. Stage D4 — 加密存储（**设计先行，不抢跑**）

**目标**：私有内容本地静态加密 +「我共享什么、对谁共享」可控。

- **D4a 设计 spike**（先拍板，产出决策文档 → 见 [PHASE_D4_ENCRYPTION_SPIKE.md](PHASE_D4_ENCRYPTION_SPIKE.md)）：
  在「整 store FS 加密 / 逐 blob convergent / 私有加密+公有明文」三条路里选定，并定密钥来源。
- **D4b 实现**：按选定模型落地，feature-gated；私有/公有分流与 UI 开关。

**风险**：高（设计张力）。**可发布**：D4a=文档，D4b=功能。

---

## 6. Stage D5 — 里程碑验证（退出 Phase D）

**退出判据**（路线图原文）：普通 PC/NAS 装上后，可作为**可寻址、可长期在线、数据加密自主**的
个人节点稳定运行数周。

**度量**：在线时长 / 自愈次数 / NAT 后被连成功率 / 内容完整性 / 常驻内存。

**已交付（自检工具）**：
- `scripts/d5-selfcheck.sh` —— 可跑自检，核验**能自动化**的判据（构建/lint、身份稳定、自愈默认、
  路由韧性；`--iroh` 加核验内容完整性/生命周期/两节点互传/keep-alive；`--full` 跑全量单测）。
- [PHASE_D5_CHECKLIST.md](PHASE_D5_CHECKLIST.md) —— 判据→核验映射 + **手动/长期观察步骤**
  （长跑数周、自愈实测、NAT 可达性、常驻内存、OS 全盘加密）+ 收官勾选表。

```bash
bash scripts/d5-selfcheck.sh --iroh   # 自动化部分全 PASS 即达标（本机实测 10/10）
```

---

## 7. 贯穿全程的纪律

| 原则 | 含义 |
|------|------|
| 节点无关 | 身份/健康/生命周期做在 `Backend` 缝上，两后端受益 |
| 加法式零回归 | 默认行为不变；新能力可开可关 |
| 每步可停可交付 | 任一 Stage 都是诚实、好用的产物 |
| 红线 | Phase E（钱包/代币）在 D 稳固前不碰 |
| 诚实标注 | n0 relay 依赖、内容寻址与加密的张力，写进文档不藏 |

---

## 8. 进度

- [x] Stage D1 — 身份层（`identity.rs` + `get_node_identity` / `set_node_label` / `export_identity` + 前端身份卡）
- [x] Stage D2 核心 — 关窗→托盘常驻 + Kubo 崩溃自愈（默认构建「可长期在线」已成立）
- [x] Stage D2 余项 — iroh 节点生命周期（shutdown→重启）+ 内容 keep-alive（tags）（feature 构建）
- [x] Stage D3 — 可观测性（节点健康度：uptime + 仓库 + 连接 + 收发字节 + iroh 内容数）
- [ ] Stage D4a — 加密存储设计 spike（文档已出，待决策）
- [ ] Stage D4b — 加密存储实现
- [x] Stage D5 自检工具 — `scripts/d5-selfcheck.sh` + `PHASE_D5_CHECKLIST.md`（自动化部分本机 10/10 PASS）
- [ ] Stage D5 里程碑达成 — 手动/长期观察项（长跑数周 / NAT 可达 / 常驻内存 / OS 加密）待现场核验
