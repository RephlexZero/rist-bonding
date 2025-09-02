use network_sim::qdisc::QdiscManager;
use network_sim::{apply_network_params, remove_network_params, NetworkParams};

#[tokio::test]
async fn smoke_apply_and_remove_qdisc() {
    let q = QdiscManager::default();
    if !q.has_net_admin().await {
        eprintln!("skipping: NET_ADMIN not available");
        return;
    }

    // Use a likely-nonexistent iface to test early error path
    let res = apply_network_params(&q, "if_not_exist_zzz", &NetworkParams::typical()).await;
    assert!(res.is_err());

    // If a disposable interface is available in CI, set IFACE env var and test end-to-end
    if let Ok(iface) = std::env::var("NETWORK_SIM_IFACE") {
        let res = apply_network_params(&q, &iface, &NetworkParams::typical()).await;
        assert!(res.is_ok(), "apply failed: {:?}", res);
        let desc = q.describe_interface_qdisc(&iface).await.unwrap();
        assert!(desc.contains("qdisc"));
        let stats = q.get_interface_stats(&iface).await.unwrap();
        // Just assert fields are present
        let _ = (stats.sent_bytes, stats.sent_packets, stats.dropped);
        let _ = remove_network_params(&q, &iface).await;
    }
}
