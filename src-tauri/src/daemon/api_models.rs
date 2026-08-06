use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeId {
    #[serde(alias = "ID")]
    pub id: String,
    #[serde(default, alias = "PublicKey")]
    pub public_key: String,
    #[serde(default, alias = "Addresses")]
    pub addresses: Vec<String>,
    #[serde(default, alias = "AgentVersion")]
    pub agent_version: String,
    #[serde(default, alias = "ProtocolVersion")]
    pub protocol_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoStats {
    #[serde(alias = "NumObjects")]
    pub num_objects: u64,
    #[serde(alias = "RepoSize")]
    pub repo_size: u64,
    #[serde(alias = "RepoPath")]
    pub repo_path: String,
    #[serde(alias = "Version")]
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmPeers {
    #[serde(alias = "Peers")]
    pub peers: Vec<PeerInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerInfo {
    #[serde(alias = "Peer")]
    pub peer: String,
    #[serde(alias = "Addr")]
    pub addr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BandwidthStats {
    #[serde(alias = "TotalIn")]
    pub total_in: u64,
    #[serde(alias = "TotalOut")]
    pub total_out: u64,
    #[serde(alias = "RateIn")]
    pub rate_in: f64,
    #[serde(alias = "RateOut")]
    pub rate_out: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BitswapStats {
    #[serde(default, alias = "ProvideBufLen")]
    pub provide_buf_len: i64,
    #[serde(default, alias = "Wantlist")]
    pub wantlist: Vec<serde_json::Value>,
    #[serde(default, alias = "Peers")]
    pub peers: Vec<String>,
    #[serde(default, alias = "BlocksReceived")]
    pub blocks_received: u64,
    #[serde(default, alias = "DataReceived")]
    pub data_received: u64,
    #[serde(default, alias = "BlocksSent")]
    pub blocks_sent: u64,
    #[serde(default, alias = "DataSent")]
    pub data_sent: u64,
    #[serde(default, alias = "DupBlksReceived")]
    pub dup_blks_received: u64,
    #[serde(default, alias = "DupDataReceived")]
    pub dup_data_received: u64,
}

#[cfg(test)]
mod tests {
    use super::BitswapStats;

    #[test]
    fn bitswap_stats_accepts_modern_kubo_wantlist_objects() {
        let stats: BitswapStats = serde_json::from_value(serde_json::json!({
            "ProvideBufLen": 0,
            "Wantlist": [{ "/": "bafybeigdyrzt" }],
            "Peers": ["12D3KooWpeer"],
            "BlocksReceived": 4,
            "DataReceived": 1024,
            "BlocksSent": 2,
            "DataSent": 512
        }))
        .expect("modern Kubo response should deserialize");
        assert_eq!(stats.wantlist.len(), 1);
        assert_eq!(stats.blocks_received, 4);
        assert_eq!(stats.dup_data_received, 0);
    }

    #[test]
    fn bitswap_stats_accepts_partial_legacy_response() {
        let stats: BitswapStats = serde_json::from_value(serde_json::json!({
            "Wantlist": ["bafybeigdyrzt"]
        }))
        .expect("partial legacy response should deserialize");
        assert_eq!(stats.wantlist[0], "bafybeigdyrzt");
        assert_eq!(stats.data_sent, 0);
    }
}
