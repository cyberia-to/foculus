// ---
// tags: sync, rust, net
// crystal-type: source
// crystal-domain: cyber
// ---
//! A put/get round trip through `SyncNode` with peers configured — the one
//! behaviour capacity-weighted placement changes for a running node.
//!
//! The wire protocol is pull-only (`MSG_GET_CHUNK`/`MSG_CHUNK_DATA`; there is
//! no push opcode), so a peer only ever receives a shard by fetching it from
//! the node that wrote it. A `put_file` that keeps only the shards placement
//! assigns to this device therefore writes the rest nowhere at all, and the
//! file stops being rebuildable even though `put_file` reported "stored".
#![cfg(feature = "net")]

use foculus::node::SyncNode;

#[tokio::test]
async fn put_then_get_round_trips_with_peers_configured() {
    let dir = tempfile::tempdir().expect("tempdir");

    // Three peers with no declared capacity: placement resolves every device
    // to an equal weight and hands each of the four shards to a different
    // one, so this node is assigned a single shard — one short of the k = 2
    // it needs to rebuild the file. The peer strings are unreachable on
    // purpose: nothing was ever sent to them, which is the point.
    std::fs::write(
        dir.path().join("peers.json"),
        serde_json::to_string(&["peer-a", "peer-b", "peer-c"]).expect("peers json"),
    )
    .expect("write peers.json");

    let node = SyncNode::start(dir.path(), 2, 4, 0)
        .await
        .expect("start node");

    let data = b"a put must leave the file rebuildable. ".repeat(8);
    node.put_file("survivor.bin", &data).await.expect("put_file");

    let got = node
        .get_file("survivor.bin")
        .await
        .expect("get_file could not rebuild the file: shards assigned to peers went nowhere");
    assert_eq!(got, data);
}
