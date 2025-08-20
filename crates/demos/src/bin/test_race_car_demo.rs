use netlink_sim::{LinkParams, NetworkOrchestrator, TestScenario};
use scenarios::{DirectionSpec, Schedule};
use std::time::Duration;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    // Create a race car network orchestrator
    let mut orchestrator = NetworkOrchestrator::new(42);

    info!("Starting race car USB cellular simulation:");
    info!("- 4G link: 300-2000 kbps with handovers");
    info!("- 5G link: 400-2000 kbps with mobility effects");
    info!("- USB modem constraints and racing blockage patterns");

    // Create race car 4G scenario with realistic USB limitations
    let race_4g_scenario = TestScenario {
        name: "race_4g_cellular".to_string(),
        description: "Race car 4G USB modem with realistic speeds".to_string(),
        forward_params: LinkParams::builder()
            .base_delay_ms(80)
            .jitter_ms(40)
            .loss_pct(0.008) // Higher loss due to racing environment
            .rate_bps(1_500_000) // 1.5 Mbps typical for race 4G
            .bucket_bytes(32 * 1024)
            .build(),
        reverse_params: LinkParams::builder()
            .base_delay_ms(100)
            .jitter_ms(50)
            .loss_pct(0.012)
            .rate_bps(800_000) // 800 kbps uplink
            .bucket_bytes(24 * 1024)
            .build(),
        duration_seconds: Some(30),
    };

    // Create race car 5G scenario - better but still USB constrained
    let race_5g_scenario = TestScenario {
        name: "race_5g_cellular".to_string(),
        description: "Race car 5G USB modem with realistic speeds".to_string(),
        forward_params: LinkParams::builder()
            .base_delay_ms(40)
            .jitter_ms(20)
            .loss_pct(0.005) // Better than 4G
            .rate_bps(2_000_000) // 2 Mbps for race 5G (USB limited)
            .bucket_bytes(48 * 1024)
            .build(),
        reverse_params: LinkParams::builder()
            .base_delay_ms(50)
            .jitter_ms(25)
            .loss_pct(0.007)
            .rate_bps(1_200_000) // 1.2 Mbps uplink
            .bucket_bytes(36 * 1024)
            .build(),
        duration_seconds: Some(30),
    };

    // Start both links for bonding - simulating 2x4G + 2x5G setup
    let handles = orchestrator
        .start_bonding_scenarios(
            vec![race_4g_scenario.clone(), race_5g_scenario.clone()],
            5000, // RIST receiver port
        )
        .await?;

    info!("Race car bonding links started:");
    for (i, handle) in handles.iter().enumerate() {
        info!(
            "  Link {}: ingress:{} -> egress:{} (scenario: {})",
            i + 1,
            handle.ingress_port,
            handle.egress_port,
            handle.scenario.name
        );
    }

    // Test the scenarios with Markov patterns from our enhanced scheduler
    info!("Testing race car scenarios with enhanced scheduling patterns...");

    // Show how our DirectionSpec presets would work
    let race_4g_strong = DirectionSpec::race_4g_strong();
    let race_5g_moderate = DirectionSpec::race_5g_moderate();
    let race_handover = DirectionSpec::race_handover_spike();

    info!("Race car DirectionSpec presets created:");
    info!("- 4G strong: {:?}", race_4g_strong);
    info!("- 5G moderate: {:?}", race_5g_moderate);
    info!("- Handover spike: {:?}", race_handover);

    // Show race car schedule patterns
    let race_4g_markov = Schedule::race_4g_markov();
    let race_track_circuit = Schedule::race_track_circuit();

    info!("Race car Schedule patterns created:");
    info!("- 4G Markov pattern: {:?}", race_4g_markov);
    info!("- Track circuit pattern: {:?}", race_track_circuit);

    // Run the simulation for the scenario duration
    tokio::time::sleep(Duration::from_secs(10)).await;

    info!("Race car USB cellular simulation completed!");
    info!("Realistic 300-2000 kbps ranges modeled with USB constraints");

    Ok(())
}
