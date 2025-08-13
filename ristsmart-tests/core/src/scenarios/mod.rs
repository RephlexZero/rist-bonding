use anyhow::Result;

pub mod baseline;
pub mod asymmetric_bw;
pub mod high_rtt;
pub mod burst_loss;
pub mod blackhole;
pub mod time_varying;
pub mod recovery;

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum ScenarioKind {
    Baseline,
    AsymmetricBw,
    HighRtt,
    BurstLoss,
    Blackhole,
    TimeVarying,
    Recovery,
}

pub trait Scenario {
    fn name(&self) -> &str;
    fn on_tick(&mut self, elapsed: std::time::Duration) -> Result<()>;
}

pub fn select_scenario(
    kind: ScenarioKind, 
    _emu: &ristsmart_netem::EmulatorHandle, 
    _links: &[crate::emulation::LinkPorts], 
    nlinks: usize
) -> Result<Box<dyn Scenario>> {
    Ok(match kind {
        ScenarioKind::Baseline => Box::new(baseline::Baseline::new(nlinks)),
        ScenarioKind::AsymmetricBw => Box::new(asymmetric_bw::AsymmetricBw::new()),
        ScenarioKind::HighRtt => Box::new(high_rtt::HighRtt::new()),
        ScenarioKind::BurstLoss => Box::new(burst_loss::BurstLoss::new()),
        ScenarioKind::Blackhole => Box::new(blackhole::Blackhole::new()),
        ScenarioKind::TimeVarying => Box::new(time_varying::TimeVarying::new()),
        ScenarioKind::Recovery => Box::new(recovery::Recovery::new()),
    })
}
