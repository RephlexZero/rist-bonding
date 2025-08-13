use anyhow::Result;
use clap::Parser;
use gstreamer as gst;
use tracing::info;
use std::time::{Duration, Instant};

use ristsmart_tests_core as core;
use core::metrics::KpiPolicy;
use core::scenarios::{ScenarioKind, select_scenario};
use core::pipelines::{build_sender_rist, build_receiver_rist, build_sender_mock, build_receiver_mock, set_state, SampleProbes};
use core::util::{mk_results_dir, mk_run_id};
use core::emulation::{build_emulator, EmuSnapshotter};
use core::weights::IdealWeighter;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, value_enum, default_value = "baseline")]
    scenario: ScenarioKind,
    #[arg(long, default_value_t = 2)]
    links: usize,
    #[arg(long, default_value_t = 30)]
    duration_secs: u64,
    #[arg(long, default_value_t = 200)]
    sample_ms: u64,
    #[arg(long, default_value = "results")]
    outdir: String,
    #[arg(long, default_value_t = 42)]
    seed: u64,

    // Data-plane toggle. If false, we fall back to control-plane mock.
    #[arg(long, default_value_t = true)]
    use_rist: bool,

    // Efficiency factor applied to convert IP-link capacity to payload capacity (RTP/H265+RIST overhead).
    #[arg(long, default_value_t = 90)]
    efficiency_percent: u32,

    // RTT penalty factor for ideal weight computation (0 = disabled).
    #[arg(long, default_value_t = 0)]
    rtt_penalty_ms: u32,

    // Encoder baseline bitrate in kbps (dynbitrate will adjust from here).
    #[arg(long, default_value_t = 8000)]
    encoder_bitrate_kbps: u32,

    // Enable strict KPI assertions (non-zero exit on failure) for CI gating.
    #[arg(long, default_value_t = false)]
    strict: bool,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    gst::init()?;
    core::register_everything_for_tests();

    let args = Args::parse();
    let run_id = mk_run_id();
    let outdir = mk_results_dir(&args.outdir, &run_id)?;

    info!("Run {} scenario={:?} links={} dur={}s", run_id, args.scenario, args.links, args.duration_secs);

    // 1) Build emulator and forwarders
    let (emu, links) = build_emulator(args.seed, args.links)?;
    let mut emu_snap = EmuSnapshotter::new(&emu, args.links);

    // 2) Ideal weighter for comparison
    let efficiency = (args.efficiency_percent as f64) / 100.0;
    let ideal_weighter = IdealWeighter::new(efficiency, args.rtt_penalty_ms as f64);

    // 3) Pipelines
    let mut probes = SampleProbes::default();
    let rx = if args.use_rist {
        build_receiver_rist(args.links, &links, &mut probes)?
    } else {
        build_receiver_mock(&mut probes)?
    };
    let tx = if args.use_rist {
        build_sender_rist(args.links, &links, args.encoder_bitrate_kbps, &mut probes)?
    } else {
        build_sender_mock(args.encoder_bitrate_kbps, &mut probes)?
    };

    // 4) Scenario
    let mut scenario = select_scenario(args.scenario, &emu, &links, args.links)?;

    // 5) Start
    set_state(&tx, gst::State::Playing)?;
    set_state(&rx, gst::State::Playing)?;

    // 6) Sampling loop
    let start = Instant::now();
    let total = Duration::from_secs(args.duration_secs);
    let period = Duration::from_millis(args.sample_ms);
    let mut kpis = KpiPolicy::defaults_for(args.scenario);

    while start.elapsed() < total {
        std::thread::sleep(period);

        // Apply scenario changes
        scenario.on_tick(start.elapsed())?;

        // Snapshot emulator (capacities, loss, delay)
        let emu_state = emu_snap.snapshot()?;

        // Pull per-link forwarded bytes
        let link_bytes = emu_snap.link_bytes_since_last()?;

        // Pull RIST stats (per session) + dispatcher weights + dyn bitrate
        let stats = probes.poll_stats()?;

        // Compute achieved throughput at receiver (after rtph265depay)
        let achieved_bps = probes.bytes_delta_since_last() as f64 * 8.0 / period.as_secs_f64();

        // Theoretical payload throughput
        let encoder_target_bps = (stats.dyn_bitrate_kbps.unwrap_or(args.encoder_bitrate_kbps) as f64) * 1000.0;
        let caps_payload_bps: f64 = emu_state
            .capacities_mbps
            .iter()
            .map(|mbps| mbps * 1_000_000.0 * efficiency)
            .sum();
        let theoretical_bps = encoder_target_bps.min(caps_payload_bps);

        // Ideal weights vs dispatcher weights
        let ideal_weights = ideal_weighter.compute(&emu_state)?;
        probes.record_sample(start, achieved_bps, theoretical_bps, &link_bytes, &emu_state, &stats, &ideal_weights)?;
        kpis.observe(&scenario.name(), &probes)?;
    }

    // 7) Stop
    set_state(&tx, gst::State::Null)?;
    set_state(&rx, gst::State::Null)?;

    // 8) Output
    let ctx = probes.into_context(run_id.clone(), outdir.clone(), args.links, args.scenario, efficiency)?;
    core::plots::render_all(&ctx, &outdir)?;
    core::util::write_report_markdown(&ctx, &outdir, &(), start.elapsed())?;

    if args.strict {
        kpis.assert_all(&ctx)?;
    }

    info!("Done. Results at {}", outdir.display());
    Ok(())
}
