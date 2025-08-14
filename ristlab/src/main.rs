use anyhow::Result;
use netlink_sim::{Emulator, LinkParams, run_link};
use tokio::time::{Duration, sleep};

#[tokio::main]
async fn main() -> Result<()> {
    let emu = Emulator::new(42);
    let emu = std::sync::Arc::new(tokio::sync::Mutex::new(emu));

    let fwd = LinkParams {
        base_delay_ms: 35,
        jitter_ms: 8,
        loss_pct: 0.001,
        reorder_pct: 0.005,
        duplicate_pct: 0.0,
        rate_bps: 20_000_000,
        bucket_bytes: 64 * 1024,
    };
    let rev = LinkParams {
        base_delay_ms: 10,
        jitter_ms: 3,
        loss_pct: 0.0005,
        reorder_pct: 0.0,
        duplicate_pct: 0.0,
        rate_bps: 1_000_000,
        bucket_bytes: 32 * 1024,
    };

    // Link A
    run_link(6001, 6101, 5004, emu.clone(), fwd.clone(), rev.clone()).await?;
    // Link B with different characteristics
    let mut fwd_b = fwd.clone();
    let rev_b = rev.clone();
    fwd_b.base_delay_ms = 80;
    fwd_b.loss_pct = 0.005;
    fwd_b.rate_bps = 6_000_000;
    run_link(6002, 6102, 5004, emu.clone(), fwd_b, rev_b).await?;

    loop {
        sleep(Duration::from_secs(60)).await;
    }
}
