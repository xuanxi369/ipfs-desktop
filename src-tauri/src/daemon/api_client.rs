use crate::error::DaemonError;
use crate::types::AddResult;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use urlencoding::encode;

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
    #[serde(default)]
    pub public_key: String,
    #[serde(rename = "Addresses")]
    #[serde(default)]
    pub addresses: Vec<String>,
    #[serde(rename = "AgentVersion")]
    #[serde(default)]
    pub agent_version: String,
    #[serde(rename = "ProtocolVersion")]
    #[serde(default)]
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

    /// 获取节点 ID（Kubo RPC 即使只读也要求 POST）
    pub async fn id(&self) -> Result<NodeId, DaemonError> {
        let url = self.api_url("id");
        tracing::debug!("Fetching node ID from: {}", url);

        let response =
            self.client
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

        let node_id = response
            .json::<NodeId>()
            .await
            .map_err(|e| DaemonError::ApiParseError(e.to_string()))?;

        tracing::info!("Node ID: {}", node_id.id);
        Ok(node_id)
    }

    /// 获取版本信息（Kubo RPC 即使只读也要求 POST）
    pub async fn version(&self) -> Result<VersionInfo, DaemonError> {
        let url = self.api_url("version");
        tracing::debug!("Fetching version from: {}", url);

        let response =
            self.client
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

        let version = response
            .json::<VersionInfo>()
            .await
            .map_err(|e| DaemonError::ApiParseError(e.to_string()))?;

        tracing::info!("IPFS version: {}", version.version);
        Ok(version)
    }

    /// 获取仓库统计信息（Kubo RPC 即使只读也要求 POST）
    pub async fn repo_stat(&self) -> Result<RepoStats, DaemonError> {
        let url = self.api_url("repo/stat");
        tracing::debug!("Fetching repo stats from: {}", url);

        let response =
            self.client
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

        let stats = response
            .json::<RepoStats>()
            .await
            .map_err(|e| DaemonError::ApiParseError(e.to_string()))?;

        tracing::info!(
            "Repo size: {} bytes, {} objects",
            stats.repo_size,
            stats.num_objects
        );
        Ok(stats)
    }

    /// 获取 Swarm 连接的对等节点（Kubo RPC 即使只读也要求 POST）
    pub async fn swarm_peers(&self) -> Result<SwarmPeers, DaemonError> {
        let url = self.api_url("swarm/peers");
        tracing::debug!("Fetching swarm peers from: {}", url);

        let response =
            self.client
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

        let response =
            self.client
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
                        detail: err_str,
                    })
                }
            }
        }
    }

    /// 添加文件到 IPFS（写入操作，POST multipart）
    ///
    /// 将本地文件上传到 IPFS 网络，返回文件哈希。
    /// 内部走分块流式上传，避免把整个文件读进内存。
    pub async fn add_file(&self, file_path: &std::path::Path) -> Result<AddResult, DaemonError> {
        self.add_file_streaming(file_path, |_, _| {}).await
    }

    /// 流式添加文件到 IPFS，通过回调报告上传进度
    ///
    /// 分块读取本地文件并流式上传（不整文件驻留内存），
    /// 每读取一个 chunk 调用 `on_progress(sent, total)`。
    pub async fn add_file_streaming<F>(
        &self,
        file_path: &std::path::Path,
        on_progress: F,
    ) -> Result<AddResult, DaemonError>
    where
        F: Fn(u64, u64) + Send + Sync + 'static,
    {
        use futures_util::StreamExt;
        use tokio_util::io::ReaderStream;

        let url = self.api_url("add?progress=false");
        tracing::info!("Adding file to IPFS (streaming): {:?}", file_path);

        let file_name = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unnamed")
            .to_string();

        let total = tokio::fs::metadata(file_path)
            .await
            .map_err(|e| DaemonError::IoError(format!("Failed to stat file: {}", e)))?
            .len();

        let file = tokio::fs::File::open(file_path)
            .await
            .map_err(|e| DaemonError::IoError(format!("Failed to open file: {}", e)))?;

        // 分块流：每读到一块就累加已发送字节并回调
        let mut sent: u64 = 0;
        let progress_stream = ReaderStream::new(file).map(move |chunk| {
            if let Ok(ref bytes) = chunk {
                sent += bytes.len() as u64;
                on_progress(sent, total);
            }
            chunk
        });

        let body = reqwest::Body::wrap_stream(progress_stream);
        let part = reqwest::multipart::Part::stream_with_length(body, total)
            .file_name(file_name.clone())
            .mime_str("application/octet-stream")
            .map_err(|e| DaemonError::ApiError(format!("Failed to create multipart: {}", e)))?;

        let form = reqwest::multipart::Form::new().part("file", part);

        let response = self
            .client
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

        let text = response
            .text()
            .await
            .map_err(|e| DaemonError::ApiParseError(e.to_string()))?;

        // IPFS add 返回 NDJSON，取第一行
        let first_line = text.lines().next().unwrap_or(&text);
        let result: AddResult = serde_json::from_str(first_line).map_err(|e| {
            DaemonError::ApiParseError(format!("Failed to parse add result: {}", e))
        })?;

        tracing::info!("File added: {} ({})", result.name, result.hash);
        Ok(result)
    }

    /// 从 IPFS 读取文件内容（cat — 流式返回原始字节）
    ///
    /// 返回文件的完整内容。对于大文件应使用 `cat_to_file` 真流式下载到磁盘。
    pub async fn cat(&self, cid: &str) -> Result<Vec<u8>, DaemonError> {
        let url = self.api_url(&format!("cat?arg={}", encode(cid)));
        tracing::info!("Cat file: {}", cid);

        let response =
            self.client
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

        let bytes = response.bytes().await.map_err(|e| {
            DaemonError::ApiParseError(format!("Failed to read cat response: {}", e))
        })?;

        tracing::info!("Cat completed: {} bytes", bytes.len());
        Ok(bytes.to_vec())
    }

    /// 真流式下载 IPFS 文件到本地路径，通过回调报告进度
    ///
    /// 适用于大文件下载：每读到一个 chunk 立即写入目标文件句柄，
    /// **不把整个文件累积在内存中**。每写入一块调用 `on_progress(written, total)`。
    /// 返回累计写入字节数。
    pub async fn cat_to_file<F>(
        &self,
        cid: &str,
        output_path: &std::path::Path,
        on_progress: F,
    ) -> Result<u64, DaemonError>
    where
        F: Fn(u64, Option<u64>),
    {
        use futures_util::StreamExt;
        use tokio::io::AsyncWriteExt;

        let url = self.api_url(&format!("cat?arg={}", encode(cid)));
        tracing::info!("Cat stream to file: {} -> {:?}", cid, output_path);

        let response =
            self.client
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

        // 确保父目录存在，再创建目标文件句柄
        if let Some(parent) = output_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| DaemonError::IoError(format!("Failed to create output dir: {}", e)))?;
        }
        let mut file = tokio::fs::File::create(output_path)
            .await
            .map_err(|e| DaemonError::IoError(format!("Failed to create output file: {}", e)))?;

        let mut written: u64 = 0;
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk =
                chunk.map_err(|e| DaemonError::ApiParseError(format!("Stream error: {}", e)))?;
            file.write_all(&chunk)
                .await
                .map_err(|e| DaemonError::IoError(format!("Failed to write chunk: {}", e)))?;
            written += chunk.len() as u64;
            on_progress(written, total);
        }
        file.flush()
            .await
            .map_err(|e| DaemonError::IoError(format!("Failed to flush output file: {}", e)))?;

        tracing::info!(
            "Cat stream completed: {} bytes -> {:?}",
            written,
            output_path
        );
        Ok(written)
    }

    /// 获取文件大小（通过 stat 端点）
    pub async fn file_size(&self, cid: &str) -> Result<u64, DaemonError> {
        let url = self.api_url(&format!("files/stat?arg=/ipfs/{}", encode(cid)));
        tracing::debug!("Stat file: {}", cid);

        let response =
            self.client
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

        let stat: StatResult = response
            .json()
            .await
            .map_err(|e| DaemonError::ApiParseError(e.to_string()))?;

        Ok(stat.cumulative_size)
    }

    /// Pin 列表（列出所有已 pin 的内容）
    pub async fn pin_ls(&self) -> Result<PinList, DaemonError> {
        let url = self.api_url("pin/ls?type=recursive&stream-channels=true");
        tracing::info!("Listing pins...");

        let response =
            self.client
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

        let text = response
            .text()
            .await
            .map_err(|e| DaemonError::ApiParseError(e.to_string()))?;

        // Kubo versions differ: newer versions return a single object
        // `{ "Keys": { "cid": { "Type": "recursive" } } }`, while
        // some gateways return NDJSON PinEntry records. Accept both forms.
        let mut pins = Vec::new();
        if let Ok(list) = serde_json::from_str::<PinList>(&text) {
            pins = list.pins;
        } else if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
            if let Some(keys) = value.get("Keys").and_then(|v| v.as_object()) {
                pins.extend(keys.iter().map(|(cid, info)| {
                    PinEntry {
                        cid: cid.clone(),
                        pin_type: info
                            .get("Type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("recursive")
                            .to_string(),
                    }
                }));
            } else if let Ok(pin) = serde_json::from_value::<PinEntry>(value) {
                pins.push(pin);
            }
        } else {
            for line in text.lines().filter(|line| !line.trim().is_empty()) {
                if let Ok(pin) = serde_json::from_str::<PinEntry>(line) {
                    pins.push(pin);
                }
            }
        }

        tracing::info!("Found {} pins", pins.len());
        Ok(PinList { pins })
    }

    /// 添加 Pin
    pub async fn pin_add(&self, cid: &str) -> Result<PinAddResult, DaemonError> {
        let url = self.api_url(&format!("pin/add?arg={}", encode(cid)));
        tracing::info!("Pinning: {}", cid);

        let response =
            self.client
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

        let text = response
            .text()
            .await
            .map_err(|e| DaemonError::ApiParseError(e.to_string()))?;
        let lines: Vec<&str> = text.lines().collect();
        let last = lines.last().copied().unwrap_or(&text);
        let result: PinAddResult = serde_json::from_str(last).map_err(|e| {
            DaemonError::ApiParseError(format!("Failed to parse pin add result: {}", e))
        })?;

        tracing::info!("Pinned: {}", cid);
        Ok(result)
    }

    /// 移除 Pin
    pub async fn pin_rm(&self, cid: &str) -> Result<PinRmResult, DaemonError> {
        let url = self.api_url(&format!("pin/rm?arg={}", encode(cid)));
        tracing::info!("Unpinning: {}", cid);

        let response =
            self.client
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

        let text = response
            .text()
            .await
            .map_err(|e| DaemonError::ApiParseError(e.to_string()))?;
        let lines: Vec<&str> = text.lines().collect();
        let last = lines.last().copied().unwrap_or(&text);
        let result: PinRmResult = serde_json::from_str(last).map_err(|e| {
            DaemonError::ApiParseError(format!("Failed to parse pin rm result: {}", e))
        })?;

        tracing::info!("Unpinned: {}", cid);
        Ok(result)
    }

    /// 带宽统计
    pub async fn stats_bw(&self) -> Result<BandwidthStats, DaemonError> {
        let url = self.api_url("stats/bw");
        tracing::debug!("Fetching bandwidth stats...");

        let response =
            self.client
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

        let stats: BandwidthStats = response
            .json()
            .await
            .map_err(|e| DaemonError::ApiParseError(e.to_string()))?;
        Ok(stats)
    }

    /// Bitswap 统计
    pub async fn bitswap_stat(&self) -> Result<BitswapStats, DaemonError> {
        let url = self.api_url("bitswap/stat");
        tracing::debug!("Fetching bitswap stats...");

        let response =
            self.client
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

        let stats: BitswapStats = response
            .json()
            .await
            .map_err(|e| DaemonError::ApiParseError(e.to_string()))?;
        Ok(stats)
    }

    // ── IPNS 端点 ──

    /// 发布 IPNS 名称（完整参数版本）
    ///
    /// 将 CID 绑定到指定的 IPNS 密钥名称。
    pub async fn name_publish(
        &self,
        cid: &str,
        key_name: &str,
        lifetime: &str,
    ) -> Result<IpnsPublishResult, DaemonError> {
        self.name_publish_full(cid, key_name, lifetime, None, false)
            .await
    }

    /// 发布 IPNS 名称（完整参数版本）
    ///
    /// # Arguments
    /// * `cid` - 要发布的 IPFS CID
    /// * `key_name` - 使用的密钥名称
    /// * `lifetime` - 记录生命周期（例如："24h", "1h30m"）
    /// * `ipns_base` - IPNS 名称的编码基数（"b58mh" 或 "base36"）
    /// * `allow_offline` - 是否允许离线发布（不广播到 DHT）
    pub async fn name_publish_full(
        &self,
        cid: &str,
        key_name: &str,
        lifetime: &str,
        ipns_base: Option<&str>,
        allow_offline: bool,
    ) -> Result<IpnsPublishResult, DaemonError> {
        let mut query_params = vec![("arg", cid), ("key", key_name), ("lifetime", lifetime)];

        // 添加可选参数
        let ipns_base_str;
        if let Some(base) = ipns_base {
            ipns_base_str = base.to_string();
            query_params.push(("ipns-base", &ipns_base_str));
        }

        let offline_str = if allow_offline { "true" } else { "false" };
        query_params.push(("allow-offline", offline_str));

        let url = self.api_url("name/publish");
        tracing::info!(
            "IPNS publish: {} -> {} (lifetime: {}, offline: {})",
            cid,
            key_name,
            lifetime,
            allow_offline
        );

        let response = self
            .client
            .post(&url)
            .query(&query_params)
            .send()
            .await
            .map_err(|e| DaemonError::ApiConnectionFailed {
                addr: self.api_addr.clone(),
                detail: e.to_string(),
            })?;

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(DaemonError::ApiError(format!(
                "name/publish failed: {}",
                body
            )));
        }

        let result: IpnsPublishResult = response
            .json()
            .await
            .map_err(|e| DaemonError::ApiParseError(e.to_string()))?;

        tracing::info!("IPNS published: {} -> {}", result.name, result.value);
        Ok(result)
    }

    /// 解析 IPNS 名称
    pub async fn name_resolve(&self, name: &str) -> Result<IpnsResolveResult, DaemonError> {
        let url = self.api_url(&format!("name/resolve?arg={}", encode(name)));
        tracing::info!("IPNS resolve: {}", name);

        let response =
            self.client
                .post(&url)
                .send()
                .await
                .map_err(|e| DaemonError::ApiConnectionFailed {
                    addr: self.api_addr.clone(),
                    detail: e.to_string(),
                })?;

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(DaemonError::ApiError(format!(
                "name/resolve failed: {}",
                body
            )));
        }

        let result: IpnsResolveResult = response
            .json()
            .await
            .map_err(|e| DaemonError::ApiParseError(e.to_string()))?;

        tracing::info!("IPNS resolved: {} -> {}", name, result.path);
        Ok(result)
    }

    /// 生成新的 IPNS 密钥（由 Kubo 管理）
    pub async fn key_gen(&self, name: &str) -> Result<KeyGenResult, DaemonError> {
        let url = self.api_url(&format!("key/gen?arg={}&type=ed25519", encode(name)));
        tracing::info!("Key gen: {}", name);

        let response =
            self.client
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

        let result: KeyGenResult = response
            .json()
            .await
            .map_err(|e| DaemonError::ApiParseError(e.to_string()))?;

        tracing::info!("Key generated: {} -> {}", result.name, result.id);
        Ok(result)
    }

    /// 列出所有 IPNS 密钥
    pub async fn key_list(&self) -> Result<KeyListResult, DaemonError> {
        let url = self.api_url("key/list");
        tracing::debug!("Key list");

        let response =
            self.client
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

        let result: KeyListResult = response
            .json()
            .await
            .map_err(|e| DaemonError::ApiParseError(e.to_string()))?;

        tracing::info!("{} keys found", result.keys.len());
        Ok(result)
    }

    /// 删除 Kubo 管理的 IPNS 密钥
    pub async fn key_rm(&self, name: &str) -> Result<(), DaemonError> {
        let url = self.api_url(&format!("key/rm?arg={}", encode(name)));
        tracing::info!("Key rm: {}", name);

        let response =
            self.client
                .post(&url)
                .send()
                .await
                .map_err(|e| DaemonError::ApiConnectionFailed {
                    addr: self.api_addr.clone(),
                    detail: e.to_string(),
                })?;

        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(DaemonError::ApiError(format!("key/rm failed: {}", body)));
        }

        tracing::info!("Key removed: {}", name);
        Ok(())
    }
    // ════════════════════════════════════════════════════════════════
    // MFS (Mutable File System) API
    // ════════════════════════════════════════════════════════════════

    /// 列出 MFS 目录内容
    pub async fn files_ls(&self, path: &str) -> Result<MfsLsResult, DaemonError> {
        let url = self.api_url("files/ls");
        let path_encoded = encode(path);

        tracing::debug!("Listing MFS directory: {}", path);

        let response = self
            .client
            .post(&url)
            .query(&[("arg", path_encoded.as_ref()), ("long", "true")])
            .send()
            .await
            .map_err(|e| DaemonError::ApiConnectionFailed {
                addr: self.api_addr.clone(),
                detail: e.to_string(),
            })?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(DaemonError::ApiError(format!(
                "files/ls failed: {}",
                error_text
            )));
        }

        let result = response
            .json::<MfsLsResult>()
            .await
            .map_err(|e| DaemonError::ApiParseError(e.to_string()))?;

        Ok(result)
    }

    /// 获取 MFS 文件/目录状态
    pub async fn files_stat(&self, path: &str) -> Result<MfsStatResult, DaemonError> {
        let url = self.api_url("files/stat");
        let path_encoded = encode(path);

        tracing::debug!("Getting MFS stat for: {}", path);

        let response = self
            .client
            .post(&url)
            .query(&[("arg", path_encoded.as_ref())])
            .send()
            .await
            .map_err(|e| DaemonError::ApiConnectionFailed {
                addr: self.api_addr.clone(),
                detail: e.to_string(),
            })?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(DaemonError::ApiError(format!(
                "files/stat failed: {}",
                error_text
            )));
        }

        let result = response
            .json::<MfsStatResult>()
            .await
            .map_err(|e| DaemonError::ApiParseError(e.to_string()))?;

        Ok(result)
    }

    /// 创建 MFS 目录
    pub async fn files_mkdir(&self, path: &str, parents: bool) -> Result<(), DaemonError> {
        let url = self.api_url("files/mkdir");
        let path_encoded = encode(path);

        tracing::debug!("Creating MFS directory: {} (parents: {})", path, parents);

        let response = self
            .client
            .post(&url)
            .query(&[
                ("arg", path_encoded.as_ref()),
                ("parents", if parents { "true" } else { "false" }),
            ])
            .send()
            .await
            .map_err(|e| DaemonError::ApiConnectionFailed {
                addr: self.api_addr.clone(),
                detail: e.to_string(),
            })?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(DaemonError::ApiError(format!(
                "files/mkdir failed: {}",
                error_text
            )));
        }

        Ok(())
    }

    /// 删除 MFS 文件/目录
    pub async fn files_rm(&self, path: &str, recursive: bool) -> Result<(), DaemonError> {
        let url = self.api_url("files/rm");
        let path_encoded = encode(path);

        tracing::debug!("Removing MFS path: {} (recursive: {})", path, recursive);

        let response = self
            .client
            .post(&url)
            .query(&[
                ("arg", path_encoded.as_ref()),
                ("recursive", if recursive { "true" } else { "false" }),
            ])
            .send()
            .await
            .map_err(|e| DaemonError::ApiConnectionFailed {
                addr: self.api_addr.clone(),
                detail: e.to_string(),
            })?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(DaemonError::ApiError(format!(
                "files/rm failed: {}",
                error_text
            )));
        }

        Ok(())
    }

    /// 复制 IPFS 对象到 MFS
    pub async fn files_cp(&self, source: &str, dest: &str) -> Result<(), DaemonError> {
        let url = self.api_url("files/cp");
        let source_encoded = encode(source);
        let dest_encoded = encode(dest);

        tracing::debug!("Copying to MFS: {} -> {}", source, dest);

        let response = self
            .client
            .post(&url)
            .query(&[
                ("arg", source_encoded.as_ref()),
                ("arg", dest_encoded.as_ref()),
            ])
            .send()
            .await
            .map_err(|e| DaemonError::ApiConnectionFailed {
                addr: self.api_addr.clone(),
                detail: e.to_string(),
            })?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(DaemonError::ApiError(format!(
                "files/cp failed: {}",
                error_text
            )));
        }

        Ok(())
    }

    /// 移动/重命名 MFS 文件/目录
    pub async fn files_mv(&self, source: &str, dest: &str) -> Result<(), DaemonError> {
        let url = self.api_url("files/mv");
        let source_encoded = encode(source);
        let dest_encoded = encode(dest);

        tracing::debug!("Moving in MFS: {} -> {}", source, dest);

        let response = self
            .client
            .post(&url)
            .query(&[
                ("arg", source_encoded.as_ref()),
                ("arg", dest_encoded.as_ref()),
            ])
            .send()
            .await
            .map_err(|e| DaemonError::ApiConnectionFailed {
                addr: self.api_addr.clone(),
                detail: e.to_string(),
            })?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(DaemonError::ApiError(format!(
                "files/mv failed: {}",
                error_text
            )));
        }

        Ok(())
    }

    /// 从 MFS 读取文件内容
    pub async fn files_read(&self, path: &str) -> Result<Vec<u8>, DaemonError> {
        let url = self.api_url("files/read");
        let path_encoded = encode(path);

        tracing::debug!("Reading MFS file: {}", path);

        let response = self
            .client
            .post(&url)
            .query(&[("arg", path_encoded.as_ref())])
            .send()
            .await
            .map_err(|e| DaemonError::ApiConnectionFailed {
                addr: self.api_addr.clone(),
                detail: e.to_string(),
            })?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(DaemonError::ApiError(format!(
                "files/read failed: {}",
                error_text
            )));
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| DaemonError::ApiError(e.to_string()))?;

        Ok(bytes.to_vec())
    }

    /// 写入内容到 MFS 文件
    pub async fn files_write(
        &self,
        path: &str,
        content: Vec<u8>,
        create: bool,
        truncate: bool,
    ) -> Result<(), DaemonError> {
        let url = self.api_url("files/write");
        let path_encoded = encode(path);

        tracing::debug!("Writing to MFS file: {} ({} bytes)", path, content.len());

        let form =
            reqwest::multipart::Form::new().part("data", reqwest::multipart::Part::bytes(content));

        let response = self
            .client
            .post(&url)
            .query(&[
                ("arg", path_encoded.as_ref()),
                ("create", if create { "true" } else { "false" }),
                ("truncate", if truncate { "true" } else { "false" }),
            ])
            .multipart(form)
            .send()
            .await
            .map_err(|e| DaemonError::ApiConnectionFailed {
                addr: self.api_addr.clone(),
                detail: e.to_string(),
            })?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            return Err(DaemonError::ApiError(format!(
                "files/write failed: {}",
                error_text
            )));
        }

        Ok(())
    }
}

/// MFS 目录列表结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MfsLsResult {
    #[serde(rename = "Entries")]
    pub entries: Option<Vec<MfsEntry>>,
}

/// MFS 目录条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MfsEntry {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Type")]
    pub entry_type: i32, // 0 = file, 1 = directory
    #[serde(rename = "Size")]
    pub size: u64,
    #[serde(rename = "Hash")]
    pub hash: String,
}

/// MFS 文件/目录状态结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MfsStatResult {
    #[serde(rename = "Hash")]
    pub hash: String,
    #[serde(rename = "Size")]
    pub size: u64,
    #[serde(rename = "CumulativeSize")]
    pub cumulative_size: u64,
    #[serde(rename = "Blocks")]
    pub blocks: u64,
    #[serde(rename = "Type")]
    pub file_type: String, // "file" or "directory"
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
