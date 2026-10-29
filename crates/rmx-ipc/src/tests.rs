use std::time::Duration;

use tokio::io::AsyncWriteExt;

use crate::bridge::bridge;
use crate::endpoint::validate_name;

#[test]
fn rejects_names_that_could_escape_the_directory() {
    for bad in ["", "../x", "a/b", ".hidden", "a b", &"x".repeat(65)] {
        assert!(validate_name(bad).is_err(), "{bad:?}");
    }
    for good in ["default", "work-2", "a.b_c"] {
        validate_name(good).unwrap();
    }
}

#[tokio::test]
async fn bridge_closes_both_directions_on_garbage() {
    let (a, mut b) = tokio::io::duplex(1024);
    let (tx, mut rx) = bridge::<_, rmx_proto::ClientMsg, rmx_proto::ServerMsg>(a, 8, 8);
    // A valid length prefix followed by a body that isn't a ClientMsg.
    b.write_all(&[1, 0, 0, 0, 0xff]).await.unwrap();
    assert!(rx.recv().await.is_none());
    // The writer was killed too, so sends start failing once it's gone.
    tokio::time::timeout(Duration::from_secs(2), tx.closed())
        .await
        .unwrap();
}
