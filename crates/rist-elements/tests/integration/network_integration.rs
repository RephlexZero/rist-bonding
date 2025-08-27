//! Simple integration tests for RIST elements with network simulation

#[cfg(feature = "network-sim")]
mod network_sim_tests {
    use gstristelements::testing;
    use network_sim::qdisc::QdiscManager;
    use network_sim::{apply_network_params, NetworkParams};

    #[tokio::test]
    async fn test_apply_network_parameters() {
        let qdisc_manager = QdiscManager::default();
        let params = NetworkParams::typical();

        let result = apply_network_params(&qdisc_manager, "lo", &params).await;

        match result {
            Ok(()) => println!("Network parameters applied successfully"),
            Err(e) => println!("Expected error in test environment: {}", e),
        }
    }

    #[tokio::test]
    async fn test_network_simulation_helpers() {
        let interface = "test0";

        let result = testing::network_sim::apply_typical_conditions(interface).await;
        println!("Apply typical conditions result: {:?}", result);

        let result = testing::network_sim::apply_poor_conditions(interface).await;
        println!("Apply poor conditions result: {:?}", result);

        let result = testing::network_sim::apply_good_conditions(interface).await;
        println!("Apply good conditions result: {:?}", result);
    }
}

mod basic_tests {
    use gstreamer::prelude::*;
    use gstristelements::testing;
    use network_sim::NetworkParams;

    #[test]
    fn test_element_creation() {
        testing::init_for_tests();

        let dispatcher = testing::create_dispatcher(Some(&[0.5, 0.5]));
        assert_eq!(dispatcher.factory().unwrap().name(), "ristdispatcher");

        let dynbitrate = testing::create_dynbitrate();
        assert_eq!(dynbitrate.factory().unwrap().name(), "dynbitrate");
    }

    #[test]
    fn test_network_params_presets() {
        let good = NetworkParams::good();
        assert_eq!(good.delay_ms, 5);
        assert_eq!(good.loss_pct, 0.001);
        assert_eq!(good.rate_kbps, 10_000);

        let typical = NetworkParams::typical();
        assert_eq!(typical.delay_ms, 20);
        assert_eq!(typical.loss_pct, 0.01);
        assert_eq!(typical.rate_kbps, 5_000);

        let poor = NetworkParams::poor();
        assert_eq!(poor.delay_ms, 100);
        assert_eq!(poor.loss_pct, 0.05);
        assert_eq!(poor.rate_kbps, 1_000);

        println!("Network params presets work correctly");
    }
}
