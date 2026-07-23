use serde::{Deserialize, Serialize};

/// IPFS 守护进程状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "data")]
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

impl Default for DaemonStatus {
    fn default() -> Self {
        DaemonStatus::Stopped
    }
}

/// IPFS API 响应 - 版本信息
#[derive(Debug, Deserialize, Serialize)]
pub struct IpfsVersion {
    #[serde(rename = "Version")]
    pub version: String,
    #[serde(rename = "Commit")]
    pub commit: String,
}

/// IPFS API 响应 - 节点 ID
#[derive(Debug, Deserialize, Serialize)]
pub struct IpfsId {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "Addresses")]
    pub addresses: Vec<String>,
}

/// IPFS API 响应 - 添加文件结果
#[derive(Debug, Deserialize, Serialize)]
pub struct AddResult {
    #[serde(rename = "Hash")]
    pub hash: String,
    #[serde(rename = "Size")]
    pub size: String,
    #[serde(rename = "Name")]
    pub name: String,
}

/// IPFS API 响应 - 节点列表
#[derive(Debug, Deserialize, Serialize)]
pub struct PeerList {
    #[serde(rename = "Peers")]
    pub peers: Vec<Peer>,
}

/// IPFS API 响应 - 单个节点信息
#[derive(Debug, Deserialize, Serialize)]
pub struct Peer {
    #[serde(rename = "Peer")]
    pub peer: String,
    #[serde(rename = "Addr")]
    pub addr: String,
}
