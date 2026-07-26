//! Kubo Backend 适配器 — Phase 4
//!
//! 将现有的 IpfsApiClient (HTTP → Go Kubo) 包装为 Backend trait 实现。
//! 这是当前默认后端，保持 100% 向后兼容。

use async_trait::async_trait;
use crate::daemon::IpfsApiClient;
use crate::backend_trait::{
    Backend, BackendType, BackendCapabilities, BackendError, BackendErrorKind,
    NodeInfo, RepoInfo, PeerInfo as BPeerInfo,
    AddOutput, PinEntry as BPinEntry,
    BandwidthInfo, BitswapInfo, IpnsOutput, IpnsPath,
};
use std::path::Path;

/// Kubo HTTP API 后端适配器
#[derive(Clone)]
pub struct KuboBackend {
    client: IpfsApiClient,
}

impl KuboBackend {
    pub fn new(api_addr: String) -> Self {
        Self {
            client: IpfsApiClient::new(api_addr),
        }
    }

    /// 从错误类型转换
    fn map_err(&self, e: crate::error::DaemonError) -> BackendError {
        match &e {
            crate::error::DaemonError::BinaryNotFound => {
                BackendError::unavailable(e.to_string())
            }
            crate::error::DaemonError::ApiConnectionFailed { .. } => {
                BackendError::network(e.to_string())
            }
            crate::error::DaemonError::ApiError(msg) if msg.contains("not found") => {
                BackendError::not_found(e.to_string())
            }
            _ => BackendError::internal(e.to_string()),
        }
    }
}

#[async_trait]
impl Backend for KuboBackend {
    fn backend_type(&self) -> BackendType {
        BackendType::Kubo
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            backend_type: BackendType::Kubo,
            ipns: true,
            pinning: true,
            gc: true,
            pubsub: true,
            mfs: true,
            bitswap: true,
            cid_version: 1,
        }
    }

    async fn is_available(&self) -> bool {
        self.client.is_reachable().await
    }

    async fn node_info(&self) -> Result<NodeInfo, BackendError> {
        let id = self.client.id().await.map_err(|e| self.map_err(e))?;
        Ok(NodeInfo {
            peer_id: id.id,
            agent_version: id.agent_version,
            protocol_version: id.protocol_version,
            addresses: id.addresses,
        })
    }

    async fn version(&self) -> Result<String, BackendError> {
        let v = self.client.version().await.map_err(|e| self.map_err(e))?;
        Ok(v.version)
    }

    async fn repo_stat(&self) -> Result<RepoInfo, BackendError> {
        let stats = self.client.repo_stat().await.map_err(|e| self.map_err(e))?;
        Ok(RepoInfo {
            num_objects: stats.num_objects,
            repo_size: stats.repo_size,
            version: stats.version,
        })
    }

    async fn repo_gc(&self) -> Result<(), BackendError> {
        self.client.repo_gc().await.map_err(|e| self.map_err(e))
    }

    async fn add_file(&self, path: &Path) -> Result<AddOutput, BackendError> {
        let result = self.client.add_file(path).await.map_err(|e| self.map_err(e))?;
        Ok(AddOutput {
            cid: result.hash,
            size: result.size.parse().unwrap_or(0),
            name: result.name,
        })
    }

    async fn cat(&self, cid: &str) -> Result<Vec<u8>, BackendError> {
        self.client.cat(cid).await.map_err(|e| self.map_err(e))
    }

    async fn file_size(&self, cid: &str) -> Result<u64, BackendError> {
        self.client.file_size(cid).await.map_err(|e| self.map_err(e))
    }

    async fn pin_ls(&self) -> Result<Vec<BPinEntry>, BackendError> {
        let list = self.client.pin_ls().await.map_err(|e| self.map_err(e))?;
        Ok(list.pins.into_iter().map(|p| BPinEntry {
            cid: p.cid,
            pin_type: p.pin_type,
        }).collect())
    }

    async fn pin_add(&self, cid: &str) -> Result<(), BackendError> {
        self.client.pin_add(cid).await.map_err(|e| self.map_err(e))?;
        Ok(())
    }

    async fn pin_rm(&self, cid: &str) -> Result<(), BackendError> {
        self.client.pin_rm(cid).await.map_err(|e| self.map_err(e))?;
        Ok(())
    }

    async fn swarm_peers(&self) -> Result<Vec<BPeerInfo>, BackendError> {
        let peers = self.client.swarm_peers().await.map_err(|e| self.map_err(e))?;
        Ok(peers.peers.into_iter().map(|p| BPeerInfo {
            peer_id: p.peer,
            address: p.addr,
            direction: None,
        }).collect())
    }

    async fn bandwidth_stats(&self) -> Result<BandwidthInfo, BackendError> {
        let bw = self.client.stats_bw().await.map_err(|e| self.map_err(e))?;
        Ok(BandwidthInfo {
            total_in: bw.total_in,
            total_out: bw.total_out,
            rate_in: bw.rate_in,
            rate_out: bw.rate_out,
        })
    }

    async fn bitswap_stats(&self) -> Result<BitswapInfo, BackendError> {
        let bs = self.client.bitswap_stat().await.map_err(|e| self.map_err(e))?;
        Ok(BitswapInfo {
            blocks_received: bs.blocks_received,
            blocks_sent: bs.blocks_sent,
            data_received: bs.data_received,
            data_sent: bs.data_sent,
        })
    }

    async fn name_publish(
        &self, cid: &str, key_name: &str, lifetime: &str,
    ) -> Result<IpnsOutput, BackendError> {
        let result = self.client.name_publish(cid, key_name, lifetime)
            .await.map_err(|e| self.map_err(e))?;
        Ok(IpnsOutput { name: result.name, value: result.value })
    }

    async fn name_resolve(&self, name: &str) -> Result<IpnsPath, BackendError> {
        let result = self.client.name_resolve(name)
            .await.map_err(|e| self.map_err(e))?;
        Ok(IpnsPath { path: result.path })
    }

    async fn shutdown(&self) -> Result<(), BackendError> {
        self.client.shutdown().await.map_err(|e| self.map_err(e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_kubo_backend_creation() {
        let backend = KuboBackend::new("http://127.0.0.1:5001".to_string());
        assert_eq!(backend.backend_type(), BackendType::Kubo);
        let caps = backend.capabilities();
        assert!(caps.ipns);
        assert!(caps.pinning);
    }

    #[tokio::test]
    async fn test_kubo_backend_unavailable() {
        let backend = KuboBackend::new("http://127.0.0.1:59999".to_string());
        assert!(!backend.is_available().await);
    }

    #[tokio::test]
    async fn test_kubo_backend_live() {
        let backend = KuboBackend::new("http://127.0.0.1:5001".to_string());
        if backend.is_available().await {
            let info = backend.node_info().await;
            assert!(info.is_ok(), "node_info should work: {:?}", info.err());
            println!("Node: {}", info.unwrap().peer_id);
        } else {
            println!("SKIP: Kubo not running");
        }
    }
}
