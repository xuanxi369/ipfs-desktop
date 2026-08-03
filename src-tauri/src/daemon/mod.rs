//! IPFS 守护进程管理模块
//!
//! 负责查找、启动、停止和监控 Kubo (go-ipfs) 守护进程

mod api_client;
mod binary;
mod controller;
mod kubo_hashes;

pub use api_client::{
    BandwidthStats, BitswapStats, IpfsApiClient, IpnsPublishResult, IpnsResolveResult, KeyEntry,
    KeyGenResult, KeyListResult, MfsEntry, MfsLsResult, MfsStatResult, NodeId, PeerInfo,
    PinAddResult, PinEntry, PinList, PinRmResult, RepoStats, SwarmPeers, VersionInfo,
};
pub use binary::BinaryFinder;
pub use controller::DaemonController;
pub use kubo_hashes::KuboHashes;
