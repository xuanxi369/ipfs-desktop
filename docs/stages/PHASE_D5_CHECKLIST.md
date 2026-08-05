# Phase D5 里程碑核对清单 —— 可信个人节点

> 承接 [PHASE_D_PLAN.md](PHASE_D_PLAN.md) §6。路线图 D 的退出判据：
> **普通 PC/NAS 装上后，可作为「可寻址、可长期在线、数据加密自主」的个人节点，稳定运行数周。**
>
> 本清单把该判据拆成可核验条目。**能自动化的**由 `scripts/d5-selfcheck.sh` 跑；
> **须长期/手动观察的**在下方「手动清单」列出步骤。

---

## 一、如何运行自动化自检

```bash
# 默认构建的可核验项（快：构建 + lint + 身份/自愈/路由）
bash scripts/d5-selfcheck.sh

# 额外核验 iroh 原生节点（内容完整性 / 生命周期 / 两节点互传 / keep-alive）
bash scripts/d5-selfcheck.sh --iroh

# 额外跑「全部默认单测」
bash scripts/d5-selfcheck.sh --full
```

> Windows 用 Git Bash 运行；脚本会自动从 `$HOME/.cargo/bin` 找 cargo。
> 退出码：全部通过 = 0，有失败 = 1，缺 cargo = 2。

---

## 二、判据 → 核验映射

| D5 判据 | 核验方式 | 自动化？ |
|---------|----------|:-------:|
| **内容完整性**（add→cat 逐字节一致） | `test_iroh_add_cat_roundtrip_integrity`（iroh）/ Kubo 需守护进程实跑 | ✅ `--iroh` |
| **身份可寻址且稳定** | `identity::*` + `test_iroh_identity_persists_across_restart` | ✅ |
| **长驻—自愈机制存在且默认开** | `test_auto_restart_serde_default` + 健康监控（退避/上限/清零） | ✅（机制）/ ⬜（实测见手动） |
| **生命周期**（关闭→自动重启，内容留存） | `test_iroh_shutdown_and_reinit` | ✅ `--iroh` |
| **跨节点可寻址**（真实 QUIC 互传） | `test_iroh_two_node_transfer` | ✅ `--iroh` |
| **内容长期保留**（免 GC） | `test_iroh_keep_and_unkeep` | ✅ `--iroh` |
| **双栈韧性**（fallback / 内容发现） | `backend_router::*`（本地+网络 fallback、自愈回填） | ✅ |
| **可观测**（在线时长/服务量/内容数） | `get_node_health` + 仪表盘「健康度」卡 | ✅（编译/命令）/ ⬜（读数见手动） |
| **代码健康** | `cargo build` + `clippy -D warnings` | ✅ |
| 长期在线（数周） | 手动观察 | ⬜ |
| NAT 后被连成功率 | 手动（跨网络 ticket 收取） | ⬜ |
| 常驻内存稳定 | 手动（长跑监测） | ⬜ |
| 数据加密自主 | OS 全盘加密（非应用级，见 spike） | ⬜ |

---

## 三、手动 / 长期观察清单

脚本跑不了的判据，按下列步骤人工核验，勾选记录：

### ⬜ 3.1 长期在线（数周）
- 启动应用并 `Start Daemon`，让其常驻。
- 每隔几天在仪表盘「节点健康度」卡记录 **应用运行 / 节点在线** 时长。
- **达标**：连续运行 ≥ 2–3 周无需人工干预。

### ⬜ 3.2 自愈实测（崩溃自动重启）
1. 应用运行、守护进程 Running。
2. 从任务管理器 / `pkill ipfs` 强杀 `ipfs` 进程（模拟崩溃）。
3. 观察：状态短暂变 Starting，随后自动回到 Running（日志有 `Auto-restarting daemon (attempt n/5)`）。
- **达标**：偶发崩溃能自动恢复；连续崩溃 5 次后停手（防崩溃循环），健康 30s 后预算清零。
- 关：若 `config.auto_restart=false` 则不自愈（预期）。

### ⬜ 3.3 关窗常驻
- 点窗口关闭按钮 → 应用不退出，隐藏到系统托盘；守护进程/节点继续运行。
- 托盘「Show Window」可唤回；托盘「Quit」才真正退出。

### ⬜ 3.4 NAT 后可达性
1. A 机（本节点，`--iroh` 构建）`iroh_add_file` + `iroh_share` 得到 ticket。
2. **从另一网络**的 B 机用该 ticket `iroh_fetch_ticket` 收取。
3. **达标**：即使 A 在 NAT 后，B 仍能经 iroh relay/打洞收到内容一致。

### ⬜ 3.5 常驻内存
- 长跑期间用任务管理器 / `ps -o rss` 每日记录进程内存。
- **达标**：内存无持续增长（无明显泄漏），维持在合理区间。

### ⬜ 3.6 数据加密自主
- **本项目刻意不做应用级加密**（理由见 [PHASE_D4_ENCRYPTION_SPIKE.md](PHASE_D4_ENCRYPTION_SPIKE.md)）。
- 核验：确认操作系统已启用**全盘加密**（Windows BitLocker / macOS FileVault / Linux LUKS）。
- 未来可选的「private 可见性标记」在 spike 里有设计，尚未实现。

---

## 四、退出判据（Phase D 收官）

全部满足即视为 Phase D 完成：

- [ ] 自动化自检 `bash scripts/d5-selfcheck.sh --iroh` **全 PASS**
- [ ] 3.1 长期在线 ≥ 2–3 周
- [ ] 3.2 自愈实测通过
- [ ] 3.3 关窗常驻通过
- [ ] 3.4 NAT 后可达（跨网络 ticket 收取成功）
- [ ] 3.5 常驻内存稳定
- [ ] 3.6 OS 全盘加密已启用
