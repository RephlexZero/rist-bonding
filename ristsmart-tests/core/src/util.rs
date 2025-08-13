use anyhow::Result;
use chrono::Utc;
use crate::metrics::RunContext;

pub fn mk_run_id() -> String {
    Utc::now().format("%Y%m%d-%H%M%S").to_string()
}

pub fn mk_results_dir(base: &str, run_id: &str) -> Result<std::path::PathBuf> {
    let dir = std::path::PathBuf::from(base).join(run_id);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn now_ms() -> u64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap();
    now.as_millis() as u64
}

pub fn write_report_markdown(
    ctx: &RunContext, 
    outdir: &std::path::Path, 
    _args: &(), // Placeholder for args - will need to be properly typed later
    dur: std::time::Duration
) -> Result<()> {
    let mut md = String::new();
    md.push_str(&format!("# RIST bonding E2E run {}\n\n", ctx.run_id));
    md.push_str(&format!(
        "- Scenario: {:?}\n- Links: {}\n- Duration: {:.1}s\n- Efficiency: {:.0}%\n", 
        ctx.scenario, 
        ctx.links, 
        dur.as_secs_f64(), 
        ctx.efficiency * 100.0
    ));
    
    // Add basic statistics
    if !ctx.metrics.samples.is_empty() {
        let achieved: Vec<f64> = ctx.metrics.samples.iter().map(|s| s.achieved_bps).collect();
        let theoretical: Vec<f64> = ctx.metrics.samples.iter().map(|s| s.theoretical_bps).collect();
        
        let mean_achieved = achieved.iter().sum::<f64>() / achieved.len() as f64;
        let mean_theoretical = theoretical.iter().sum::<f64>() / theoretical.len() as f64;
        let mean_ratio = if mean_theoretical > 0.0 {
            mean_achieved / mean_theoretical
        } else { 0.0 };

        md.push_str("\n## Summary Statistics\n\n");
        md.push_str(&format!("- Mean achieved throughput: {:.1} Mbps\n", mean_achieved / 1_000_000.0));
        md.push_str(&format!("- Mean theoretical throughput: {:.1} Mbps\n", mean_theoretical / 1_000_000.0));
        md.push_str(&format!("- Mean efficiency ratio: {:.2}\n", mean_ratio));
        md.push_str(&format!("- Total samples: {}\n", ctx.metrics.samples.len()));
    }
    
    md.push_str("\n## Throughput\n\n");
    md.push_str("![Total throughput](throughput_total.png)\n\n");
    md.push_str("\n## Weights\n\n");
    md.push_str("![Dispatcher vs Ideal](weights_overlay.png)\n");
    
    std::fs::write(outdir.join("report.md"), md)?;
    Ok(())
}

// Helper function for creating temporary test Args in other modules
#[cfg(test)]
pub fn create_test_args() -> () {
    ()
}
