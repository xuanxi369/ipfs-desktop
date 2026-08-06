#![cfg(feature = "iroh-backend")]

use ipfs_desktop_rust_lib::backend_trait::Backend;
use ipfs_desktop_rust_lib::iroh_adapter::IrohBackend;
use std::time::Duration;

/// Exercises the real iroh transport between two independent local nodes.
/// This is an integration target so CI cannot accidentally satisfy it with
/// the default-feature stub implementation.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_local_nodes_exchange_content_by_ticket() {
    let dir_a = tempfile::tempdir().expect("node A temp directory");
    let dir_b = tempfile::tempdir().expect("node B temp directory");
    let node_a = IrohBackend::new(dir_a.path().to_owned());
    let node_b = IrohBackend::new(dir_b.path().to_owned());

    let payload: Vec<u8> = (0..64_000u32).map(|n| (n % 251) as u8).collect();
    let source = dir_a.path().join("payload.bin");
    tokio::fs::write(&source, &payload)
        .await
        .expect("write source payload");

    let added = node_a.add_file(&source).await.expect("node A add");
    let ticket = node_a
        .share_ticket(&added.cid)
        .await
        .expect("node A share ticket");

    let received = tokio::time::timeout(Duration::from_secs(45), node_b.fetch_ticket(&ticket))
        .await
        .expect("two-node transfer timed out")
        .expect("node B fetch ticket");
    assert_eq!(received, payload, "network payload must be byte-identical");

    let local = node_b.cat(&added.cid).await.expect("node B local cat");
    assert_eq!(local, payload, "fetched blob must persist in node B store");

    node_b.shutdown().await.expect("node B shutdown");
    node_a.shutdown().await.expect("node A shutdown");
}
