#!/usr/bin/env bash
# ─────────────────────────────────────────────────────────────────────────
# Phase D5 里程碑自检 —— 验证「可信个人节点」中**可自动核验**的部分
#
# 用法：
#   bash scripts/d5-selfcheck.sh            # 默认构建的核验项（快）
#   bash scripts/d5-selfcheck.sh --iroh     # 额外跑 iroh 原生节点核验（较慢，需网络栈）
#   bash scripts/d5-selfcheck.sh --full     # 额外跑「全部默认单测」（~2 分钟）
#
# Windows：用 Git Bash 运行。cargo 会自动从 $HOME/.cargo/bin 找。
#
# 脚本只覆盖**能自动核验**的 D5 判据；「长跑数周 / NAT 可达性 / 常驻内存 / 加密」
# 等观察项由 PHASE_D5_CHECKLIST.md 的手动清单负责，脚本末尾会打印提示。
# ─────────────────────────────────────────────────────────────────────────
set -u

export PATH="$HOME/.cargo/bin:$PATH"
MANIFEST="src-tauri/Cargo.toml"
LOG="$(mktemp 2>/dev/null || echo /tmp/d5_selfcheck.log)"

RUN_IROH=0
RUN_FULL=0
for arg in "$@"; do
  case "$arg" in
    --iroh) RUN_IROH=1 ;;
    --full) RUN_FULL=1 ;;
    -h|--help) grep '^#' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
  esac
done

if ! command -v cargo >/dev/null 2>&1; then
  echo "✗ 找不到 cargo。请安装 Rust（https://rustup.rs）或确认 \$HOME/.cargo/bin 在 PATH。"
  exit 2
fi

pass=0; fail=0; skip=0
declare -a ROWS

# run <criterion> <label> <cmd...>
run() {
  local crit="$1"; local label="$2"; shift 2
  printf '  ▶ %-42s ' "$label"
  if "$@" >"$LOG" 2>&1; then
    echo "PASS"
    ROWS+=("✅|$crit|$label")
    pass=$((pass+1))
  else
    echo "FAIL"
    ROWS+=("❌|$crit|$label")
    echo "    ── 末尾输出 ──"
    tail -6 "$LOG" | sed 's/^/    /'
    fail=$((fail+1))
  fi
}

note_skip() { # <criterion> <label> <why>
  ROWS+=("⚠️|$1|$2（$3）")
  skip=$((skip+1))
}

echo "═══════════════════════════════════════════════════════════════"
echo " Phase D5 里程碑自检 —— 可信个人节点（自动化部分）"
echo "═══════════════════════════════════════════════════════════════"

echo
echo "【1】代码与构建健康"
run "地基" "构建健康 (cargo build)"                 cargo build   --manifest-path "$MANIFEST" -q
run "地基" "Lint 健康 (clippy -D warnings)"          cargo clippy  --manifest-path "$MANIFEST" -q -- -D warnings -A deprecated

echo
echo "【2】可信节点能力（默认构建）"
run "身份稳定" "身份跨重启稳定 (identity persist)"    cargo test --manifest-path "$MANIFEST" --lib identity:: -q
run "长驻自愈" "自愈默认开 (auto_restart serde 默认)" cargo test --manifest-path "$MANIFEST" --lib config::tests::test_auto_restart_serde_default -q
run "双栈韧性" "路由/fallback 决策 (router)"          cargo test --manifest-path "$MANIFEST" --lib backend_router:: -q

if [ "$RUN_FULL" -eq 1 ]; then
  echo
  echo "【2b】全部默认单测（comprehensive，~2 分钟）"
  run "综合" "全部默认单测" cargo test --manifest-path "$MANIFEST" --lib -q
else
  note_skip "综合" "全部默认单测" "加 --full 运行"
fi

echo
echo "【3】iroh 原生节点能力（feature 构建）"
if [ "$RUN_IROH" -eq 1 ]; then
  F=(--features iroh-backend)
  run "内容完整性" "iroh add→cat 逐字节一致"          cargo test --manifest-path "$MANIFEST" "${F[@]}" --lib test_iroh_add_cat_roundtrip_integrity -q -- --test-threads=1
  run "身份稳定"   "iroh 身份跨重启"                   cargo test --manifest-path "$MANIFEST" "${F[@]}" --lib test_iroh_identity_persists_across_restart -q -- --test-threads=1
  run "生命周期"   "iroh shutdown→自动重启+内容留存"   cargo test --manifest-path "$MANIFEST" "${F[@]}" --lib test_iroh_shutdown_and_reinit -q -- --test-threads=1
  run "可寻址"     "iroh 两节点 QUIC 互传"             cargo test --manifest-path "$MANIFEST" "${F[@]}" --lib test_iroh_two_node_transfer -q -- --test-threads=1
  run "内容保留"   "iroh keep-alive (tag)"             cargo test --manifest-path "$MANIFEST" "${F[@]}" --lib test_iroh_keep_and_unkeep -q -- --test-threads=1
else
  note_skip "iroh 能力" "iroh 原生节点核验" "加 --iroh 运行"
fi

# ── 结果汇总 ──
echo
echo "═══════════════════════════════════════════════════════════════"
echo " 结果汇总"
echo "═══════════════════════════════════════════════════════════════"
printf ' %-4s %-12s %s\n' "" "判据" "核验项"
for r in "${ROWS[@]}"; do
  IFS='|' read -r icon crit label <<< "$r"
  printf ' %-3s %-12s %s\n' "$icon" "$crit" "$label"
done
echo "───────────────────────────────────────────────────────────────"
echo " PASS: $pass   FAIL: $fail   SKIP: $skip"

# ── 手动观察项提示 ──
cat <<'MANUAL'

───────────────────────────────────────────────────────────────
 以下 D5 判据**无法脚本自动化**，须手动/长期观察（详见 PHASE_D5_CHECKLIST.md）：

  ⬜ 长期在线：连续运行数周，记录在线时长（仪表盘「节点健康度」卡）
  ⬜ 自愈实测：运行中 kill 掉 ipfs 守护进程，观察是否自动重启（日志/状态）
  ⬜ 关窗常驻：关闭窗口后从系统托盘确认节点仍在运行
  ⬜ NAT 可达性：从另一网络用 ticket 收取本节点内容，验证被连成功
  ⬜ 常驻内存：长跑期间用任务管理器/`ps` 记录内存是否稳定
  ⬜ 数据加密：确认已启用 OS 全盘加密（BitLocker/FileVault/LUKS）
              —— 本项目刻意不做应用级加密，见 PHASE_D4_ENCRYPTION_SPIKE.md
───────────────────────────────────────────────────────────────
MANUAL

rm -f "$LOG" 2>/dev/null
[ "$fail" -eq 0 ] && exit 0 || exit 1
