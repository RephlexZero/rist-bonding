//! Integration tests for netns-testbench
//!
//! These tests verify testbench-specific functionality that isn't covered by
//! the scenarios crate's own tests.

/// Initialize logging for tests
fn init_logging() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("netns_testbench=debug")
        .try_init();
}

#[tokio::test]
async fn test_crate_imports_and_orchestrator() {
    init_logging();

    // Test that we can import and use the crate types
    use netns_testbench::{NetworkOrchestrator, TestbenchError};

    // Test error types specific to testbench
    let error = TestbenchError::InvalidConfig("test configuration error".to_string());
    assert!(error.to_string().contains("test configuration error"));

    // Test orchestrator creation (core functionality of this crate)
    let result = NetworkOrchestrator::new(12345).await;
    match result {
        Ok(orchestrator) => {
            println!("✅ NetworkOrchestrator created successfully");

            // Test that orchestrator has expected properties
            // Note: We can't test apply_scenario without proper network privileges
            drop(orchestrator);
        }
        Err(e) => {
            println!(
                "NetworkOrchestrator creation failed (expected in CI without privileges): {}",
                e
            );

            // Verify it's the expected permission error, not a code error
            assert!(
                e.to_string().contains("permission")
                    || e.to_string().contains("Operation not permitted")
                    || e.to_string().contains("capability")
                    || e.to_string().contains("namespace"),
                "Error should be permission-related, got: {}",
                e
            );
        }
    }

    println!("✅ Testbench-specific functionality verified");
}

#[tokio::test]
async fn test_testbench_error_variants() {
    init_logging();

    use netns_testbench::TestbenchError;

    // Test error variants specific to testbench
    let errors = vec![TestbenchError::InvalidConfig("config error".to_string())];

    for error in errors {
        // Ensure each error can be displayed and debugged
        let _display = error.to_string();
        let _debug = format!("{:?}", error);

        // Ensure error implements required traits
        let _: Box<dyn std::error::Error> = Box::new(error);
    }

    println!("✅ TestbenchError variants work correctly");
}

#[tokio::test]
async fn test_scenario_integration_with_testbench() {
    init_logging();

    // Test that testbench can work with scenarios (integration point)
    // This tests the interface between scenarios crate and testbench, not scenario validity

    let scenarios = scenarios::Presets::basic_scenarios();
    assert!(
        !scenarios.is_empty(),
        "Should have basic scenarios available for testbench"
    );

    // Test that a basic scenario has the structure expected by testbench
    if let Some(scenario) = scenarios.first() {
        assert!(
            !scenario.name.is_empty(),
            "Scenario name required for testbench"
        );
        assert!(
            !scenario.links.is_empty(),
            "Links required for testbench operation"
        );

        // Test that links have the structure testbench expects
        for link in &scenario.links {
            assert!(!link.name.is_empty(), "Link name required for testbench");
            assert!(
                !link.a_ns.is_empty(),
                "Source namespace required for testbench"
            );
            assert!(
                !link.b_ns.is_empty(),
                "Destination namespace required for testbench"
            );

            // Testbench needs at least one direction to be configured
            assert!(
                match &link.a_to_b {
                    scenarios::Schedule::Constant(_) => true,
                    scenarios::Schedule::Steps(_) => true,
                    _ => false,
                } || match &link.b_to_a {
                    scenarios::Schedule::Constant(_) => true,
                    scenarios::Schedule::Steps(_) => true,
                    _ => false,
                },
                "Testbench requires at least one direction to be configured"
            );
        }
    }

    println!("✅ Scenario integration with testbench verified");
}
