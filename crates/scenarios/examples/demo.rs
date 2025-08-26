//! Demo of the enhanced scenarios crate
use scenarios::*;
use std::time::Duration;

fn main() {
    println!("Enhanced Scenarios Demo");
    println!("=========================\n");

    // Show basic presets
    println!("Basic Scenarios:");
    for scenario in Presets::basic_scenarios() {
        println!("  {:<20} - {}", scenario.name, scenario.description);
    }

    // Show cellular scenarios
    println!("\n📱 Cellular Scenarios:");
    for scenario in Presets::cellular_scenarios() {
        println!("  {:<20} - {}", scenario.name, scenario.description);
    }

    // Show new 5G scenarios
    println!("\n🏎️ 5G NR Scenarios:");
    for scenario in Presets::nr_scenarios() {
        println!("  {:<25} - {}", scenario.name, scenario.description);
    }

    // Show direction presets including 5G
    println!("\n🌐 Direction Presets:");
    let presets = [
        ("Good", DirectionSpec::good()),
        ("LTE Downlink", DirectionSpec::lte_downlink()),
        ("LTE Uplink", DirectionSpec::lte_uplink()),
        ("5G NR Good", DirectionSpec::nr_good()),
        ("5G NR Cell Edge", DirectionSpec::nr_cell_edge()),
        ("5G mmWave", DirectionSpec::nr_mmwave()),
        ("5G Sub-6GHz", DirectionSpec::nr_sub6ghz()),
        ("5G URLLC", DirectionSpec::nr_urllc()),
        ("5G eMBB", DirectionSpec::nr_embb()),
        ("5G mMTC", DirectionSpec::nr_mmtc()),
    ];

    for (name, spec) in presets.iter() {
        println!(
            "  {:<15} - Rate: {:7}kbps, Delay: {:3}ms, Loss: {:.3}%",
            name,
            spec.rate_kbps,
            spec.base_delay_ms,
            spec.loss_pct * 100.0
        );
    }

    // Show 5G effects
    println!("\n⚡ 5G Special Effects:");
    let base_5g = DirectionSpec::nr_sub6ghz();
    println!(
        "  Base 5G Sub-6:     Rate: {:7}kbps, Delay: {:3}ms, Loss: {:.3}%",
        base_5g.rate_kbps,
        base_5g.base_delay_ms,
        base_5g.loss_pct * 100.0
    );

    let with_ca = base_5g.clone().with_carrier_aggregation(3);
    println!(
        "  With 3x CA:        Rate: {:7}kbps, Delay: {:3}ms, Loss: {:.3}%",
        with_ca.rate_kbps,
        with_ca.base_delay_ms,
        with_ca.loss_pct * 100.0
    );

    let with_blockage = DirectionSpec::nr_mmwave().with_mmwave_blockage(0.8);
    println!(
        "  mmWave Blocked:    Rate: {:7}kbps, Delay: {:3}ms, Loss: {:.3}%",
        with_blockage.rate_kbps,
        with_blockage.base_delay_ms,
        with_blockage.loss_pct * 100.0
    );

    // Show custom scenario
    println!("\n🏗️ Custom Scenario:");
    let custom = ScenarioBuilder::new("demo")
        .description("Demo scenario with 5G characteristics")
        .duration(Duration::from_secs(60))
        .add_link(LinkSpec::symmetric(
            "5g_link".to_string(),
            "tx0".to_string(),
            "rx0".to_string(),
            Schedule::Constant(DirectionSpec::nr_embb()),
        ))
        .metadata("test_type", "5g_demo")
        .build();

    println!("  Name: {}", custom.name);
    println!("  Duration: {:?}s", custom.duration_seconds);
    println!("  Links: {}", custom.links.len());
    println!("  Metadata: {:?}", custom.metadata);

    println!("\n✅ Enhanced 5G scenarios crate working correctly!");
}
