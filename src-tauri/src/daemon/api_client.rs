use serde::{Deserialize, Serialize};
use reqwest::Client;
use std::time::Duration;
use crate::error::DaemonError;
use crate::types::AddResult;

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
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "PublicKey")]
    pub public_key: String,
    #[serde(rename = "Addresses")]
    pub addresses: Vec<String>,
    #[serde(rename = "AgentVersion")]
    pub agent_version: String,
    #[serde(rename = "ProtocolVersion")]
    pub protocol_version: String,
}

/// 仓库统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoStats {
    #[serde(rename = "NumObjects")]
    pub num_objects: u64,
    #[serde(rename = "RepoSize")]
    pub repo_size: u64,
    #[serde(rename = "RepoPath")]
    pub repo_path: String,
    #[serde(rename = "Version")]
    pub version: String,
}

/// 版本信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    #[serde(rename = "Version")]
    pub version: String,
    #[serde(rename = "Commit")]
    pub commit: String,
    #[serde(rename = "Repo")]
    pub repo: String,
    #[serde(rename = "System")]
    pub system: String,
    #[serde(rename = "Golang")]
    pub golang: String,
}

/// Swarm 连接信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmPeers {
    #[serde(rename = "Peers")]
    pub peers: Vec<PeerInfo>,
}

/// 对等节点信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    #[serde(rename = "Peer")]
    pub peer: String,
    #[serde(rename = "Addr")]
    pub addr: String,
}

/// Pin 列表
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinList {
    #[serde(rename = "Pins")]
    #[serde(default)]
    pub pins: Vec<PinEntry>,
}

/// 单个 Pin 条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinEntry {
    #[serde(rename = "Cid")]
    pub cid: String,
    #[serde(rename = "Type")]
    pub pin_type: String,
}

/// Pin 添加结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinAddResult {
    #[serde(rename = "Pins")]
    pub pins: Vec<String>,
}

/// Pin 移除结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinRmResult {
    #[serde(rename = "Pins")]
    pub pins: Vec<String>,
}

/// 带宽统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandwidthStats {
    #[serde(rename = "TotalIn")]
    pub total_in: u64,
    #[serde(rename = "TotalOut")]
    pub total_out: u64,
    #[serde(rename = "RateIn")]
    pub rate_in: f64,
    #[serde(rename = "RateOut")]
    pub rate_out: f64,
}

/// Bitswap 统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BitswapStats {
    #[serde(rename = "ProvideBufLen")]
    pub provide_buf_len: i64,
    #[serde(rename = "Wantlist")]
    #[serde(default)]
    pub wantlist: Vec<String>,
    #[serde(rename = "Peers")]
    #[serde(default)]
    pub peers: Vec<String>,
    #[serde(rename = "BlocksReceived")]
    pub blocks_received: u64,
    #[serde(rename = "DataReceived")]
    pub data_received: u64,
    #[serde(rename = "BlocksSent")]
    pub blocks_sent: u64,
    #[serde(rename = "DataSent")]
    pub data_sent: u64,
    #[serde(rename = "DupBlksReceived")]
    pub dup_blks_received: u64,
    #[serde(rename = "DupDataReceived")]
    pub dup_data_received: u64,
}

/// IPNS 发布结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpnsPublishResult {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Value")]
    pub value: String,
}

/// IPNS 解析结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpnsResolveResult {
    #[serde(rename = "Path")]
    pub path: String,
}

/// 密钥生成结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyGenResult {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Id")]
    pub id: String,
}

/// 密钥列表
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyListResult {
    #[serde(rename = "Keys")]
    pub keys: Vec<KeyEntry>,
}

/// 单个密钥条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyEntry {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Id")]
    pub id: String,
}

impl IpfsApiClient {
    /// 创建新的 API 客户端
    pub fn new(api_addr: String) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        Self { client, api_addr }
    }

    /// 获取 API 基础 URL
    fn api_url(&self, endpoint: &str) -> String {
        format!("{}/api/v0/{}", self.api_addr, endpoint)
    }

    /// 获取节点 ID（只读，GET）
    pub async fn id(&self) -> Result<NodeId, DaemonError> {
        let url = self.api_url("id");
        tracing::debug!("Fetching node ID from: {}", url);

        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| DaemonError::ApiConnectionFailed {
                addr: self.api_addr.clone(),
                detail: e.to_string(),
            })?;

        if !response.status().is_success() {
            return Err(DaemonError::ApiError(format!(
                "API returned error status: {}",
                response.status()
            )));
        }

        let node_id = response
            .json::<NodeId>()
            .await
            .map_err(|e| DaemonError::ApiParseError(e.to_string()))?;

        tracing::info!("Node ID: {}", node_id.id);
        Ok(node_id)
    }

    /// 获取版本信息（只读，GET）
    pub async fn version(&self) -> Result<VersionInfo, DaemonError> {
        let url = self.api_url("version");
        tracing::debug!("Fetching version from: {}", url);

        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| DaemonError::ApiConnectionFailed {
                addr: self.api_addr.clone(),
                detail: e.to_string(),
            })?;

        if !response.status().is_success() {
            return Err(DaemonError::ApiError(format!(
                "API returned error status: {}",
                response.status()
            )));
        }

        let version = response
            .json::<VersionInfo>()
            .await
            .map_err(|e| DaemonError::ApiParseError(e.to_string()))?;

        tracing::info!("IPFS version: {}", version.version);
        Ok(version)
    }

    /// 获取仓库统计信息（只读，GET）
    pub async fn repo_stat(&self) -> Result<RepoStats, DaemonError> {
        let url = self.api_url("repo/stat");
        tracing::debug!("Fetching repo stats from: {}", url);

        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| DaemonError::ApiConnectionFailed {
                addr: self.api_addr.clone(),
                detail: e.to_string(),
            })?;

        if !response.status().is_success() {
            return Err(DaemonError::ApiError(format!(
                "API returned error status: {}",
                response.status()
            )));
        }

        let stats = response
            .json::<RepoStats>()
            .await
            .map_err(|e| DaemonError::ApiParseError(e.to_string()))?;

        tracing::info!("Repo size: {} bytes, {} objects", stats.repo_size, stats.num_objects);
        Ok(stats)
    }

    /// 获取 Swarm 连接的对等节点（只读，GET）
    pub async fn swarm_peers(&self) -> Result<SwarmPeers, DaemonError> {
        let url = self.api_url("swarm/peers");
        tracing::debug!("Fetching swarm peers from: {}", url);

        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| DaemonError::ApiConnectionFailed {
                addr: self.api_addr.clone(),
                detail: e.to_string(),
            })?;

        if !response.status().is_success() {
            return Err(DaemonError::ApiError(format!(
                "API returned error status: {}",
                response.status()
            )));
        }

        let peers = response
            .json::<SwarmPeers>()
            .await
            .map_err(|e| DaemonError::ApiParseError(e.to_string()))?;

        tracing::info!("Connected to {} peers", peers.peers.len());
        Ok(peers)
    }

    /// 检查 API 是否可达
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

    /// 运行垃圾回收（写入操作，POST）
    pub async fn repo_gc(&self) -> Result<(), DaemonError> {
        let url = self.api_url("repo/gc");
        tracing::info!("Starting garbage collection...");

        let response = self.client
            .post(&url)
            .send()
            .await
            .map_err(|e| DaemonError::ApiConnectionFailed {
                addr: self.api_addr.clone(),
                detail: e.to_string(),
            })?;

        if !response.status().is_success() {
            return Err(DaemonError::ApiError(format!(
                "API returned error status: {}",
                response.status()
            )));
        }

        tracing::info!("Garbage collection completed");
        Ok(())
    }

    /// 关闭守护进程（写入操作，POST）
    pub async fn shutdown(&self) -> Result<(), DaemonError> {
        let url = self.api_url("shutdown");
        tracing::info!("Sending shutdown command to daemon...");

        match self.client.post(&url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    tracing::info!("Daemon shutdown command sent successfully");
                    Ok(())
                } else {
                    Err(DaemonError::ApiError(format!(
                        "API returned error status: {}",
                        response.status()
                    )))
                }
            }
            Err(e) => {
                let err_str = e.to_string();
                if err_str.contains("connection") || err_str.contains("closed") {
                    tracing::info!("Daemon is shutting down (connection closed)");
                    Ok(())
                } else {
                    Err(DaemonError::ApiConnectionFailed {
                        addr: self.api_addr.clone(),
                        source: err_str,
                    })
                }
            }
        }
    }

    /// 添加文件到 IPFS（写入操作，POST multipart）
    ///
    /// 将本地文件上传到 IPFS 网络，返回文件哈希。
    pub async fn add_file(&self, file_path: &std::path::Path) -> Result<AddResult, DaemonError> {
        let url = self.api_url("add?progress=false");
        tracing::info!("Adding file to IPFS: {:?}", file_path);

        let file_name = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unnamed")
            .to_string();

        let file_bytes = tokio::fs::read(file_path).await
            .map_err(|e| DaemonError::IoError(format!("Failed to read file: {}", e)))?;

        let part = reqwest::multipart::Part::bytes(file_bytes)
            .file_name(file_name.clone())
            .mime_str("application/octet-stream")
            .map_err(|e| DaemonError::ApiError(format!("Failed to create multipart: {}", e)))?;

        let form = reqwest::multipart::Form::new()
            .part("file", part);

        let response = self.client
            .post(&url)
            .multipart(form)
            .send()
            .await
            .map_err(|e| DaemonError::ApiConnectionFailed {
                addr: self.api_addr.clone(),
                detail: e.to_string(),
            })?;

        if !response.status().is_success() {
            return Err(DaemonError::ApiError(format!(
                "API returned error status: {}",
                response.status()
            )));
        }

        let text = response.text().await
            .map_err(|e| DaemonError::ApiParseError(e.to_string()))?;

        // IPFS add 返回 NDJSON，取第一行
        let first_line = text.lines().next().unwrap_or(&text);
        let result: AddResult = serde_json::from_str(first_line)
            .map_err(|e| DaemonError::ApiParseError(format!("Failed to parse add result: {}", e)))?;

        tracing::info!("File added: {} ({})", result.name, result.hash);
        Ok(result)
    }

    /// 列出目录中的文件（只读，GET）
    pub async fn ls(&self, cid: &str) -> Result<serde_json::Value, DaemonError> {
        let url = self.api_url(&format!("ls?arg={}", cid));
        tracing::debug!("Listing: {}", url);

        let response = self.client
            .post(&url)
            .send()
            .await
            .map_err(|e| DaemonError::ApiConnectionFailed {
                addr: self.api_addr.clone(),
                detail: e.to_string(),
            })?;

        if !response.status().is_success() {
            return Err(DaemonError::ApiError(format!(
                "API returned error status: {}",
                response.status()
            )));
        }

        let value = response.json().await
            .map_err(|e| DaemonError::ApiParseError(e.to_string()))?;
        Ok(value)
    }

    /// 从 IPFS 读取文件内容（cat — 流式返回原始字节）
    ///
    /// 返回文件的完整内容。对于大文件建议使用 `cat_stream` 分段读取。
    pub async fn cat(&self, cid: &str) -> Result<Vec<u8>, DaemonError> {
        let url = self.api_url(&format!("cat?arg={}", cid));
        tracing::info!("Cat file: {}", cid);

        let response = self.client
            .post(&url)
            .send()
            .await
            .map_err(|e| DaemonError::ApiConnectionFailed {
                addr: self.api_addr.clone(),
                detail: e.to_string(),
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(DaemonError::ApiError(format!(
                "cat failed ({}): {}",
                status, body
            )));
        }

        let bytes = response.bytes().await
            .map_err(|e| DaemonError::ApiParseError(format!("Failed to read cat response: {}", e)))?;

        tracing::info!("Cat completed: {} bytes", bytes.len());
        Ok(bytes.to_vec())
    }

    /// 流式读取 IPFS 文件内容，通过回调报告进度
    ///
    /// 适用于大文件下载。每读取一个 chunk 调用 `on_progress(loaded, total)`。
    pub async fn cat_stream(
        &self,
        cid: &str,
        on_progress: impl Fn(u64, Option<u64>),
    ) -> Result<Vec<u8>, DaemonError> {
        let url = self.api_url(&format!("cat?arg={}", cid));
        tracing::info!("Cat stream: {}", cid);

        let response = self.client
            .post(&url)
            .send()
            .await
            .map_err(|e| DaemonError::ApiConnectionFailed {
                addr: self.api_addr.clone(),
                detail: e.to_string(),
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(DaemonError::ApiError(format!(
                "cat failed ({}): {}",
                status, body
            )));
        }

        let total = response.content_length();
        let mut data = Vec::new();
        let mut stream = response.bytes_stream();

        use futures_util::StreamExt;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| {
                DaemonError::ApiParseError(format!("Stream error: {}", e))
            })?;
            data.extend_from_slice(&chunk);
            on_progress(data.len() as u64, total);
        }

        tracing::info!("Cat stream completed: {} bytes", data.len());
        Ok(data)
    }

    /// 下载 IPFS 文件到本地路径
    ///
    /// 使用 ipfs get 将文件/dir 保存到本地。
    /// 注意：IPFS HTTP API 的 /get 返回 tar 流，这里直接保存到指定路径。
    pub async fn get(&self, cid: &str, output_path: &std::path::Path) -> Result<(), DaemonError> {
        let url = self.api_url(&format!("get?arg={}&archive=true", cid));
        tracing::info!("Get file: {} -> {:?}", cid, output_path);

        let response = self.client
            .post(&url)
            .send()
            .await
            .map_err(|e| DaemonError::ApiConnectionFailed {
                addr: self.api_addr.clone(),
                detail: e.to_string(),
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(DaemonError::ApiError(format!(
                "get failed ({}): {}",
                status, body
            )));
        }

        let bytes = response.bytes().await
            .map_err(|e| DaemonError::ApiParseError(format!("Failed to read get response: {}", e)))?;

        // 确保父目录存在
        if let Some(parent) = output_path.parent() {
            tokio::fs::create_dir_all(parent).await
                .map_err(|e| DaemonError::IoError(format!("Failed to create output dir: {}", e)))?;
        }

        tokio::fs::write(output_path, &bytes).await
            .map_err(|e| DaemonError::IoError(format!("Failed to write output file: {}", e)))?;

        tracing::info!("Get completed: {} bytes -> {:?}", bytes.len(), output_path);
        Ok(())
    }

    /// 获取文件大小（通过 stat 端点）
    pub async fn file_size(&self, cid: &str) -> Result<u64, DaemonError> {
        let url = self.api_url(&format!("files/stat?arg=/ipfs/{}", cid));
        tracing::debug!("Stat file: {}", cid);

        let response = self.client
            .post(&url)
            .send()
            .await
            .map_err(|e| DaemonError::ApiConnectionFailed {
                addr: self.api_addr.clone(),
                detail: e.to_string(),
            })?;

        if !response.status().is_success() {
            return Err(DaemonError::ApiError(format!(
                "stat failed: {}",
                response.status()
            )));
        }

        #[derive(Deserialize)]
        struct StatResult {
            #[serde(rename = "CumulativeSize")]
            cumulative_size: u64,
        }

        let stat: StatResult = response.json().await
            .map_err(|e| DaemonError::ApiParseError(e.to_string()))?;

        Ok(stat.cumulative_size)
    }

    /// Pin 列表（列出所有已 pin 的内容）
    pub async fn pin_ls(&self) -> Result<PinList, DaemonError> {
        let url = self.api_url("pin/ls?type=recursive&stream-channels=true");
        tracing::info!("Listing pins...");

        let response = self.client
            .post(&url)
            .send()
            .await
            .map_err(|e| DaemonError::ApiConnectionFailed {
                addr: self.api_addr.clone(),
                detail: e.to_string(),
            })?;

        if !response.status().is_success() {
            return Err(DaemonError::ApiError(format!(
                "pin ls failed: {}",
                response.status()
            )));
        }

        let text = response.text().await
            .map_err(|e| DaemonError::ApiParseError(e.to_string()))?;

        // pin/ls 返回 NDJSON，逐行解析
        let mut pins = Vec::new();
        for line in text.lines() {
            if line.trim().is_empty() { continue; }
            if let Ok(pin) = serde_json::from_str::<PinEntry>(line) {
                pins.push(pin);
            }
        }

        tracing::info!("Found {} pins", pins.len());
        Ok(PinList { pins })
    }

    /// 添加 Pin
    pub async fn pin_add(&self, cid: &str) -> Result<PinAddResult, DaemonError> {
        let url = self.api_url(&format!("pin/add?arg={}", cid));
        tracing::info!("Pinning: {}", cid);

        let response = self.client
            .post(&url)
            .send()
            .await
            .map_err(|e| DaemonError::ApiConnectionFailed {
                addr: self.api_addr.clone(),
                detail: e.to_string(),
            })?;

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(DaemonError::ApiError(format!("pin add failed: {}", body)));
        }

        let text = response.text().await
            .map_err(|e| DaemonError::ApiParseError(e.to_string()))?;
        let lines: Vec<&str> = text.lines().collect();
        let last = lines.last().copied().unwrap_or(&text);
        let result: PinAddResult = serde_json::from_str(last)
            .map_err(|e| DaemonError::ApiParseError(format!("Failed to parse pin add result: {}", e)))?;

        tracing::info!("Pinned: {}", cid);
        Ok(result)
    }

    /// 移除 Pin
    pub async fn pin_rm(&self, cid: &str) -> Result<PinRmResult, DaemonError> {
        let url = self.api_url(&format!("pin/rm?arg={}", cid));
        tracing::info!("Unpinning: {}", cid);

        let response = self.client
            .post(&url)
            .send()
            .await
            .map_err(|e| DaemonError::ApiConnectionFailed {
                addr: self.api_addr.clone(),
                detail: e.to_string(),
            })?;

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(DaemonError::ApiError(format!("pin rm failed: {}", body)));
        }

        let text = response.text().await
            .map_err(|e| DaemonError::ApiParseError(e.to_string()))?;
        let lines: Vec<&str> = text.lines().collect();
        let last = lines.last().copied().unwrap_or(&text);
        let result: PinRmResult = serde_json::from_str(last)
            .map_err(|e| DaemonError::ApiParseError(format!("Failed to parse pin rm result: {}", e)))?;

        tracing::info!("Unpinned: {}", cid);
        Ok(result)
    }

    /// 带宽统计
    pub async fn stats_bw(&self) -> Result<BandwidthStats, DaemonError> {
        let url = self.api_url("stats/bw");
        tracing::debug!("Fetching bandwidth stats...");

        let response = self.client
            .post(&url)
            .send()
            .await
            .map_err(|e| DaemonError::ApiConnectionFailed {
                addr: self.api_addr.clone(),
                detail: e.to_string(),
            })?;

        if !response.status().is_success() {
            return Err(DaemonError::ApiError(format!(
                "stats/bw failed: {}",
                response.status()
            )));
        }

        let stats: BandwidthStats = response.json().await
            .map_err(|e| DaemonError::ApiParseError(e.to_string()))?;
        Ok(stats)
    }

    /// Bitswap 统计
    pub async fn bitswap_stat(&self) -> Result<BitswapStats, DaemonError> {
        let url = self.api_url("bitswap/stat");
        tracing::debug!("Fetching bitswap stats...");

        let response = self.client
            .post(&url)
            .send()
            .await
            .map_err(|e| DaemonError::ApiConnectionFailed {
                addr: self.api_addr.clone(),
                detail: e.to_string(),
            })?;

        if !response.status().is_success() {
            return Err(DaemonError::ApiError(format!(
                "bitswap/stat failed: {}",
                response.status()
            )));
        }

        let stats: BitswapStats = response.json().await
            .map_err(|e| DaemonError::ApiParseError(e.to_string()))?;
        Ok(stats)
    }

    // ── IPNS 端点 ──

    /// 发布 IPNS 名称
    ///
    /// 将 CID 绑定到指定的 IPNS 密钥名称。
    pub async fn name_publish(
        &self,
        cid: &str,
        key_name: &str,
        lifetime: &str,
    ) -> Result<IpnsPublishResult, DaemonError> {
        let url = self.api_url(&format!(
            "name/publish?arg={}&key={}&lifetime={}",
            cid, key_name, lifetime
        ));
        tracing::info!("IPNS publish: {} -> {}", cid, key_name);

        let response = self.client
            .post(&url)
            .send()
            .await
            .map_err(|e| DaemonError::ApiConnectionFailed {
                addr: self.api_addr.clone(),
                detail: e.to_string(),
            })?;

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(DaemonError::ApiError(format!("name/publish failed: {}", body)));
        }

        let result: IpnsPublishResult = response.json().await
            .map_err(|e| DaemonError::ApiParseError(e.to_string()))?;

        tracing::info!("IPNS published: {} -> {}", result.name, result.value);
        Ok(result)
    }

    /// 解析 IPNS 名称
    pub async fn name_resolve(&self, name: &str) -> Result<IpnsResolveResult, DaemonError> {
        let url = self.api_url(&format!("name/resolve?arg={}", name));
        tracing::info!("IPNS resolve: {}", name);

        let response = self.client
            .post(&url)
            .send()
            .await
            .map_err(|e| DaemonError::ApiConnectionFailed {
                addr: self.api_addr.clone(),
                detail: e.to_string(),
            })?;

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(DaemonError::ApiError(format!("name/resolve failed: {}", body)));
        }

        let result: IpnsResolveResult = response.json().await
            .map_err(|e| DaemonError::ApiParseError(e.to_string()))?;

        tracing::info!("IPNS resolved: {} -> {}", name, result.path);
        Ok(result)
    }

    /// 生成新的 IPNS 密钥（由 Kubo 管理）
    pub async fn key_gen(&self, name: &str) -> Result<KeyGenResult, DaemonError> {
        let url = self.api_url(&format!("key/gen?arg={}&type=ed25519", name));
        tracing::info!("Key gen: {}", name);

        let response = self.client
            .post(&url)
            .send()
            .await
            .map_err(|e| DaemonError::ApiConnectionFailed {
                addr: self.api_addr.clone(),
                detail: e.to_string(),
            })?;

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(DaemonError::ApiError(format!("key/gen failed: {}", body)));
        }

        let result: KeyGenResult = response.json().await
            .map_err(|e| DaemonError::ApiParseError(e.to_string()))?;

        tracing::info!("Key generated: {} -> {}", result.name, result.id);
        Ok(result)
    }

    /// 列出所有 IPNS 密钥
    pub async fn key_list(&self) -> Result<KeyListResult, DaemonError> {
        let url = self.api_url("key/list");
        tracing::debug!("Key list");

        let response = self.client
            .post(&url)
            .send()
            .await
            .map_err(|e| DaemonError::ApiConnectionFailed {
                addr: self.api_addr.clone(),
                detail: e.to_string(),
            })?;

        if !response.status().is_success() {
            return Err(DaemonError::ApiError(format!(
                "key/list failed: {}",
                response.status()
            )));
        }

        let result: KeyListResult = response.json().await
            .map_err(|e| DaemonError::ApiParseError(e.to_string()))?;

        tracing::info!("{} keys found", result.keys.len());
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_api_client_creation() {
        let client = IpfsApiClient::new("http://127.0.0.1:5001".to_string());
        let url = client.api_url("id");
        assert_eq!(url, "http://127.0.0.1:5001/api/v0/id");
    }

    #[tokio::test]
    async fn test_api_client_unreachable() {
        let client = IpfsApiClient::new("http://127.0.0.1:59999".to_string());
        let reachable = client.is_reachable().await;
        assert!(!reachable, "Should not be reachable on unused port");
    }

    #[tokio::test]
    async fn test_api_client_connection_error() {
        let client = IpfsApiClient::new("http://127.0.0.1:59999".to_string());
        let result = client.id().await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        match err {
            DaemonError::ApiConnectionFailed { addr, detail: _ } => {
                assert!(addr.contains("59999"));
            }
            _ => panic!("Expected ApiConnectionFailed, got {:?}", err),
        }
    }

    #[tokio::test]
    async fn test_api_client_live() {
        let client = IpfsApiClient::new("http://127.0.0.1:5001".to_string());
        let is_reachable = client.is_reachable().await;
        if is_reachable {
            println!("IPFS API is reachable — running live tests");
            match client.id().await {
                Ok(node_id) => {
                    println!("Node ID: {}", node_id.id);
                    assert!(!node_id.id.is_empty());
                }
                Err(e) => println!("Failed to get node ID (daemon might be starting): {}", e),
            }
        } else {
            println!("IPFS API is not reachable, skipping live tests");
        }
    }
}
