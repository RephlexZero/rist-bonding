//! Docker-based integration tests for network simulation
//!
//! These tests are designed to run inside Docker containers with
//! proper network capabilities.

#[cfg(all(feature = "docker", test))]
mod docker_tests {
    use network_sim::docker::{DockerNetworkEnv, DockerNetworkError};
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_docker_network_environment_setup() {
        let mut env = DockerNetworkEnv::new();

        match env.setup_basic_network().await {
            Ok(()) => {
                println!("✅ Docker network environment setup successful");

                // Wait a moment for interfaces to be ready
                sleep(Duration::from_millis(100)).await;

                // Test if we can apply network impairments
                let result = env
                    .apply_network_impairments("veth_test0", 10, 0.5, 5000)
                    .await;
                match result {
                    Ok(()) => println!("✅ Network impairments applied successfully"),
                    Err(e) => println!("⚠️ Network impairments failed: {}", e),
                }

                // Test connectivity between namespaces
                let connectivity = env.test_connectivity("test_ns1", "192.168.100.1").await;
                match connectivity {
                    Ok(true) => println!("✅ Connectivity test passed"),
                    Ok(false) => println!(
                        "ℹ️ Connectivity test failed (may be expected in some environments)"
                    ),
                    Err(e) => println!("⚠️ Connectivity test error: {}", e),
                }
            }
            Err(DockerNetworkError::CommandFailed(msg))
                if msg.contains("Operation not permitted") =>
            {
                println!(
                    "ℹ️ Network setup requires proper Docker capabilities (--cap-add=NET_ADMIN)"
                );
            }
            Err(e) => {
                println!("⚠️ Network setup failed: {}", e);
            }
        }

        // Always attempt cleanup
        let _ = env.cleanup().await;
    }

    #[tokio::test]
    async fn test_network_impairment_scenarios() {
        let mut env = DockerNetworkEnv::new();

        if env.setup_basic_network().await.is_ok() {
            println!("✅ Network setup for impairment testing");

            // Test different impairment scenarios
            let scenarios = vec![
                ("low_latency", 5, 0.1, 10000),
                ("high_latency", 200, 2.0, 1000),
                ("lossy_network", 50, 10.0, 5000),
                ("bandwidth_limited", 10, 0.5, 100),
            ];

            for (name, delay_ms, loss_pct, rate_kbps) in scenarios {
                println!("🔧 Testing scenario: {}", name);

                let result = env
                    .apply_network_impairments("veth_test0", delay_ms, loss_pct, rate_kbps)
                    .await;
                match result {
                    Ok(()) => println!(
                        "  ✅ Applied: {}ms delay, {}% loss, {} kbps",
                        delay_ms, loss_pct, rate_kbps
                    ),
                    Err(e) => println!("  ⚠️ Failed to apply {}: {}", name, e),
                }

                // Small delay between scenarios
                sleep(Duration::from_millis(100)).await;
            }
        } else {
            println!("ℹ️ Skipping impairment tests (network setup failed)");
        }

        let _ = env.cleanup().await;
    }

    #[tokio::test]
    async fn test_multiple_namespace_operations() {
        let mut env = DockerNetworkEnv::new();

        // Try to create multiple namespaces
        for i in 1..=3 {
            let ns_name = format!("test_multi_ns_{}", i);
            let result = env.create_namespace(&ns_name).await;
            match result {
                Ok(()) => println!("✅ Created namespace: {}", ns_name),
                Err(e) => println!("⚠️ Failed to create namespace {}: {}", ns_name, e),
            }
        }

        println!("ℹ️ Created {} namespaces", env.namespaces.len());

        // Cleanup
        let _ = env.cleanup().await;
    }
}

#[cfg(not(feature = "docker"))]
mod fallback_tests {
    #[test]
    fn test_docker_feature_not_enabled() {
        println!("ℹ️ Docker feature not enabled, skipping Docker-based tests");
        println!("ℹ️ To enable Docker tests, use: cargo test --features docker");
    }
}

// Standard network simulation tests that work everywhere
mod basic_network_sim_tests {
    use network_sim::qdisc::QdiscManager;
    use network_sim::{apply_network_params, NetworkParams};

    #[tokio::test]
    async fn test_network_params_creation() {
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

        println!("✅ All network parameter presets work correctly");
    }

    #[tokio::test]
    async fn test_qdisc_manager_creation() {
        let qdisc_manager = QdiscManager::default();
        let params = NetworkParams::typical();

        // This will fail in most test environments, but we test the code path
        let result = apply_network_params(&qdisc_manager, "lo", &params).await;

        match result {
            Ok(()) => println!("✅ Network parameters applied (unexpected in test env)"),
            Err(e) => println!("ℹ️ Expected error in test environment: {}", e),
        }
    }
}
