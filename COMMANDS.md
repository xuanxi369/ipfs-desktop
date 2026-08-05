# Tauri 命令目录

这是后端命令的统一索引。命令实现位于 `src-tauri/src/commands.rs` 与 `commands_mfs.rs`，注册入口位于 `src-tauri/src/lib.rs`。

| 领域 | 命令 |
|---|---|
| 守护进程 | `get_daemon_status`, `start_daemon`, `stop_daemon`, `restart_daemon` |
| 配置 | `get_config`, `update_config`, `set_auto_launch`, `get_auto_launch`, `set_binary_hash`, `get_binary_verification_info` |
| 文件 | `add_file`, `add_files`, `add_file_with_progress`, `cat_file`, `download_file`, `get_file_size` |
| 内容索引 | `list_content`, `remove_content_record` |
| Pin | `get_pin_list`, `add_pin`, `remove_pin` |
| IPNS/密钥 | `generate_key`, `list_keys`, `delete_key`, `ipns_publish`, `ipns_resolve` |
| MFS | `mfs_ls`, `mfs_stat`, `mfs_mkdir`, `mfs_rm`, `mfs_cp`, `mfs_mv`, `mfs_read`, `mfs_write` |
| 代理/离线 | `get_proxy_stats`, `set_prefetch_hint`, `get_offline_queue`, `flush_offline_queue` |
| 带宽 | `get_bandwidth_config`, `set_bandwidth_config`, `get_bandwidth_status` |
| 双后端 | `get_active_backend`, `switch_backend`, `get_backend_capabilities`, `get_route_policy`, `set_route_policy`, `get_backend_route` |
| iroh | `iroh_add_file`, `iroh_node_info`, `iroh_share`, `iroh_fetch_ticket`, `iroh_register_ticket`, `iroh_keep`, `iroh_unkeep`, `iroh_shutdown` |
| 诊断 | `run_benchmark`, `run_compat_test` |
| 身份/健康 | `get_node_identity`, `set_node_label`, `export_identity`, `get_node_health` |

前端调用约定：使用 `invoke("命令名", 参数对象)`；错误返回结构由 `src/types.ts::formatError` 统一展示。
