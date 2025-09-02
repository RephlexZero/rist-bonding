use network_sim::qdisc::QdiscManager;
use network_sim::{apply_ingress_params, remove_ingress_params, NetworkParams};

#[tokio::test]
async fn smoke_apply_and_remove_ingress() {
    let q = QdiscManager::default();
    if !q.has_net_admin().await {
        eprintln!("skipping: NET_ADMIN not available");
        return;
    }

    // Nonexistent iface should error early
    let res = apply_ingress_params(&q, "if_not_exist_zzz", &NetworkParams::typical()).await;
    assert!(res.is_err());

    if let Ok(iface) = std::env::var("NETWORK_SIM_IFACE") {
        let res = apply_ingress_params(&q, &iface, &NetworkParams::typical()).await;
        assert!(res.is_ok(), "apply ingress failed: {:?}", res);
        // Remove and ensure it doesn't error fatally
        let _ = remove_ingress_params(&q, &iface).await;
    }
}
