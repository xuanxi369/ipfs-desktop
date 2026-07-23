use serde::{Deserialize, Serialize};
use reqwest::Client;
use std::time::Duration;

/// IPFS HTTP API 客户端
/// 
/// 用于与 Kubo 守护进程的 HTTP API 进行通信
#[derive(Clone)]
pub struct IpfsApiClient {
    /// HTTP 客户端
    client: Client,
    /// API 地址（例如：http://127.0.0.1:5001）
    api_addr: String,
}

/// 节点 ID 信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeId {
    /// 节点的 Peer ID
    #[serde(rename = "ID")]
    pub id: String,
    /// 公钥
    #[serde(rename = "PublicKey")]
    pub public_key: String,
    /// 监听地址
    #[serde(rename = "Addresses")]
    pub addresses: Vec<String>,
    /// Agent 版本
    #[serde(rename = "AgentVersion")]
    pub agent_version: String,
    /// 协议版本
    #[serde(rename = "ProtocolVersion")]
    pub protocol_version: String,
}

/// 仓库统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoStats {
    /// 仓库中的对象数量
    #[serde(rename = "NumObjects")]
    pub num_objects: u64,
    /// 仓库大小（字节）
    #[serde(rename = "RepoSize")]
    pub repo_size: u64,
    /// 仓库路径
    #[serde(rename = "RepoPath")]
    pub repo_path: String,
    /// 版本
    #[serde(rename = "Version")]
    pub version: String,
}

/// 版本信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    /// 版本号
    #[serde(rename = "Version")]
    pub version: String,
    /// Commit 哈希
    #[serde(rename = "Commit")]
    pub commit: String,
    /// 仓库版本
    #[serde(rename = "Repo")]
    pub repo: String,
    /// 系统信息
    #[serde(rename = "System")]
    pub system: String,
    /// Golang 版本
    #[serde(rename = "Golang")]
    pub golang: String,
}

/// Swarm 连接信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmPeers {
    /// 对等节点列表
    #[serde(rename = "Peers")]
    pub peers: Vec<PeerInfo>,
}

/// 对等节点信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    /// Peer ID
    #[serde(rename = "Peer")]
    pub peer: String,
    /// 地址
    #[serde(rename = "Addr")]
    pub addr: String,
}

impl IpfsApiClient {
    /// 创建新的 API 客户端
    /// 
    /// # Arguments
    /// 
    /// * `api_addr` - API 地址（例如：http://127.0.0.1:5001）
    pub fn new(api_addr: String) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");
        
        Self {
            client,
            api_addr,
        }
    }
    
    /// 获取 API 基础 URL
    fn api_url(&self, endpoint: &str) -> String {
        format!("{}/api/v0/{}", self.api_addr, endpoint)
    }
    
    /// 获取节点 ID
    /// 
    /// 对应命令：ipfs id
    pub async fn id(&self) -> Result<NodeId, String> {
        let url = self.api_url("id");
        
        tracing::debug!("Fetching node ID from: {}", url);
        
        let response = self.client
            .post(&url)
            .send()
            .await
            .map_err(|e| format!("Failed to connect to IPFS API: {}", e))?;
        
        if !response.status().is_success() {
            return Err(format!("API returned error status: {}", response.status()));
        }
        
        let node_id = response
            .json::<NodeId>()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;
        
        tracing::info!("Node ID: {}", node_id.id);
        Ok(node_id)
    }
    
    /// 获取版本信息
    /// 
    /// 对应命令：ipfs version
    pub async fn version(&self) -> Result<VersionInfo, String> {
        let url = self.api_url("version");
        
        tracing::debug!("Fetching version from: {}", url);
        
        let response = self.client
            .post(&url)
            .send()
            .await
            .map_err(|e| format!("Failed to connect to IPFS API: {}", e))?;
        
        if !response.status().is_success() {
            return Err(format!("API returned error status: {}", response.status()));
        }
        
        let version = response
            .json::<VersionInfo>()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;
        
        tracing::info!("IPFS version: {}", version.version);
        Ok(version)
    }
    
    /// 获取仓库统计信息
    /// 
    /// 对应命令：ipfs repo stat
    pub async fn repo_stat(&self) -> Result<RepoStats, String> {
        let url = self.api_url("repo/stat");
        
        tracing::debug!("Fetching repo stats from: {}", url);
        
        let response = self.client
            .post(&url)
            .send()
            .await
            .map_err(|e| format!("Failed to connect to IPFS API: {}", e))?;
        
        if !response.status().is_success() {
            return Err(format!("API returned error status: {}", response.status()));
        }
        
        let stats = response
            .json::<RepoStats>()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;
        
        tracing::info!("Repo size: {} bytes, {} objects", stats.repo_size, stats.num_objects);
        Ok(stats)
    }
    
    /// 获取 Swarm 连接的对等节点
    /// 
    /// 对应命令：ipfs swarm peers
    pub async fn swarm_peers(&self) -> Result<SwarmPeers, String> {
        let url = self.api_url("swarm/peers");
        
        tracing::debug!("Fetching swarm peers from: {}", url);
        
        let response = self.client
            .post(&url)
            .send()
            .await
            .map_err(|e| format!("Failed to connect to IPFS API: {}", e))?;
        
        if !response.status().is_success() {
            return Err(format!("API returned error status: {}", response.status()));
        }
        
        let peers = response
            .json::<SwarmPeers>()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;
        
        tracing::info!("Connected to {} peers", peers.peers.len());
        Ok(peers)
    }
    
    /// 检查 API 是否可达
    /// 
    /// 通过尝试调用 /api/v0/id 来验证 API 是否可用
    pub async fn is_reachable(&self) -> bool {
        match self.id().await {
            Ok(_) => {
                tracing::debug!("IPFS API is reachable");
                true
            }
            Err(e) => {
                tracing::debug!("IPFS API is not reachable: {}", e);
                false
            }
        }
    }
    
    /// 运行垃圾回收
    /// 
    /// 对应命令：ipfs repo gc
    pub async fn repo_gc(&self) -> Result<(), String> {
        let url = self.api_url("repo/gc");
        
        tracing::info!("Starting garbage collection...");
        
        let response = self.client
            .post(&url)
            .send()
            .await
            .map_err(|e| format!("Failed to connect to IPFS API: {}", e))?;
        
        if !response.status().is_success() {
            return Err(format!("API returned error status: {}", response.status()));
        }
        
        tracing::info!("Garbage collection completed");
        Ok(())
    }
    
    /// 关闭守护进程
    /// 
    /// 对应命令：ipfs shutdown
    pub async fn shutdown(&self) -> Result<(), String> {
        let url = self.api_url("shutdown");
        
        tracing::info!("Sending shutdown command to daemon...");
        
        // 注意：shutdown 命令会导致连接中断，所以可能会返回错误
        // 我们需要处理这种情况
        match self.client
            .post(&url)
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    tracing::info!("Daemon shutdown command sent successfully");
                    Ok(())
                } else {
                    Err(format!("API returned error status: {}", response.status()))
                }
            }
            Err(e) => {
                // 连接中断可能是正常的，因为守护进程正在关闭
                let err_str = e.to_string();
                if err_str.contains("connection") || err_str.contains("closed") {
                    tracing::info!("Daemon is shutting down (connection closed)");
                    Ok(())
                } else {
                    Err(format!("Failed to send shutdown command: {}", e))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_api_client() {
        // 这个测试需要运行中的 IPFS 守护进程
        // 在 CI 环境中可能会跳过
        let client = IpfsApiClient::new("http://127.0.0.1:5001".to_string());
        
        // 测试连接
        let is_reachable = client.is_reachable().await;
        if is_reachable {
            println!("IPFS API is reachable");
            
            // 测试获取节点 ID
            match client.id().await {
                Ok(node_id) => {
                    println!("Node ID: {}", node_id.id);
                }
                Err(e) => {
                    println!("Failed to get node ID: {}", e);
                }
            }
        } else {
            println!("IPFS API is not reachable, skipping tests");
        }
    }
}
