use anyhow::Result;
use rand::{Rng, SeedableRng, rngs::StdRng};
use std::{collections::HashMap, net::SocketAddr, time::Duration};
use tokio::{
    net::UdpSocket,
    sync::{Mutex, mpsc},
    time::{Instant, sleep},
};

/// Parameters controlling behavior of a single direction of a link.
#[derive(Clone, Debug)]
pub struct LinkParams {
    pub base_delay_ms: u64,
    pub jitter_ms: u64,
    pub loss_pct: f32,
    pub reorder_pct: f32,
    pub duplicate_pct: f32,
    pub rate_bps: u64,
    pub bucket_bytes: usize,
}

impl LinkParams {
    /// Create parameters for a good quality link
    pub fn good() -> Self {
        Self {
            base_delay_ms: 10,
            jitter_ms: 2,
            loss_pct: 0.0001,
            reorder_pct: 0.0,
            duplicate_pct: 0.0,
            rate_bps: 50_000_000, // 50 Mbps
            bucket_bytes: 128 * 1024,
        }
    }

    /// Create parameters for a poor quality link
    pub fn poor() -> Self {
        Self {
            base_delay_ms: 100,
            jitter_ms: 25,
            loss_pct: 0.02,
            reorder_pct: 0.01,
            duplicate_pct: 0.001,
            rate_bps: 2_000_000, // 2 Mbps
            bucket_bytes: 32 * 1024,
        }
    }

    /// Create parameters for a typical internet connection
    pub fn typical() -> Self {
        Self {
            base_delay_ms: 35,
            jitter_ms: 8,
            loss_pct: 0.001,
            reorder_pct: 0.005,
            duplicate_pct: 0.0,
            rate_bps: 20_000_000, // 20 Mbps
            bucket_bytes: 64 * 1024,
        }
    }

    /// Create parameters for a mobile/cellular connection
    pub fn cellular() -> Self {
        Self {
            base_delay_ms: 80,
            jitter_ms: 40,
            loss_pct: 0.005,
            reorder_pct: 0.008,
            duplicate_pct: 0.0005,
            rate_bps: 10_000_000, // 10 Mbps
            bucket_bytes: 48 * 1024,
        }
    }

    /// Create parameters for a satellite link
    pub fn satellite() -> Self {
        Self {
            base_delay_ms: 300,
            jitter_ms: 50,
            loss_pct: 0.003,
            reorder_pct: 0.002,
            duplicate_pct: 0.0,
            rate_bps: 5_000_000, // 5 Mbps
            bucket_bytes: 32 * 1024,
        }
    }

    /// Builder pattern for custom link parameters
    pub fn builder() -> LinkParamsBuilder {
        LinkParamsBuilder::default()
    }
}

/// Builder for creating custom LinkParams
#[derive(Default)]
pub struct LinkParamsBuilder {
    base_delay_ms: Option<u64>,
    jitter_ms: Option<u64>,
    loss_pct: Option<f32>,
    reorder_pct: Option<f32>,
    duplicate_pct: Option<f32>,
    rate_bps: Option<u64>,
    bucket_bytes: Option<usize>,
}

impl LinkParamsBuilder {
    pub fn base_delay_ms(mut self, ms: u64) -> Self {
        self.base_delay_ms = Some(ms);
        self
    }

    pub fn jitter_ms(mut self, ms: u64) -> Self {
        self.jitter_ms = Some(ms);
        self
    }

    pub fn loss_pct(mut self, pct: f32) -> Self {
        self.loss_pct = Some(pct);
        self
    }

    pub fn reorder_pct(mut self, pct: f32) -> Self {
        self.reorder_pct = Some(pct);
        self
    }

    pub fn duplicate_pct(mut self, pct: f32) -> Self {
        self.duplicate_pct = Some(pct);
        self
    }

    pub fn rate_bps(mut self, bps: u64) -> Self {
        self.rate_bps = Some(bps);
        self
    }

    pub fn bucket_bytes(mut self, bytes: usize) -> Self {
        self.bucket_bytes = Some(bytes);
        self
    }

    pub fn build(self) -> LinkParams {
        LinkParams {
            base_delay_ms: self.base_delay_ms.unwrap_or(10),
            jitter_ms: self.jitter_ms.unwrap_or(2),
            loss_pct: self.loss_pct.unwrap_or(0.0),
            reorder_pct: self.reorder_pct.unwrap_or(0.0),
            duplicate_pct: self.duplicate_pct.unwrap_or(0.0),
            rate_bps: self.rate_bps.unwrap_or(10_000_000),
            bucket_bytes: self.bucket_bytes.unwrap_or(64 * 1024),
        }
    }
}

/// Simple token bucket rate limiter.
struct TokenBucket {
    rate_bps: u64,
    bucket: f64,
    cap: f64,
    last: Instant,
}

impl TokenBucket {
    fn new(rate_bps: u64, cap_bytes: usize) -> Self {
        Self {
            rate_bps,
            bucket: cap_bytes as f64,
            cap: cap_bytes as f64,
            last: Instant::now(),
        }
    }

    fn allow(&mut self, bytes: usize) -> bool {
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f64();
        self.last = now;
        self.bucket = (self.bucket + dt * (self.rate_bps as f64 / 8.0)).min(self.cap);
        if self.bucket >= bytes as f64 {
            self.bucket -= bytes as f64;
            true
        } else {
            false
        }
    }
}

/// Emulator maintains NAT-style mapping of reverse destinations.
#[derive(Debug)]
pub struct Emulator {
    pub reverse_dst: HashMap<u16, SocketAddr>,
    #[allow(dead_code)] // RNG doesn't implement Debug but is used internally
    pub rng: StdRng,
}

impl Emulator {
    pub fn new(seed: u64) -> Self {
        Self {
            reverse_dst: HashMap::new(),
            rng: StdRng::seed_from_u64(seed),
        }
    }
}

struct Pipe {
    params: LinkParams,
    tbf: Mutex<TokenBucket>,
}

impl Pipe {
    fn new(p: LinkParams) -> Self {
        Self {
            tbf: Mutex::new(TokenBucket::new(p.rate_bps, p.bucket_bytes)),
            params: p,
        }
    }
}

/// Worker moving packets while applying impairments.
async fn run_pipe(
    mut rx: mpsc::Receiver<(Vec<u8>, SocketAddr)>,
    send_sock: std::sync::Arc<UdpSocket>,
    default_dst: SocketAddr,
    pipe: Pipe,
    rng: Mutex<StdRng>,
) {
    let mut hold: Option<(Vec<u8>, SocketAddr)> = None;
    while let Some((buf, dst_override)) = rx.recv().await {
        let p = &pipe.params;
        let mut r = rng.lock().await;

        // Loss
        if rand::Rng::r#gen::<f32>(&mut *r) < p.loss_pct {
            continue;
        }

        // Duplication
        let dup = rand::Rng::r#gen::<f32>(&mut *r) < p.duplicate_pct;

        // Reorder (1-packet window)
        if hold.is_none() && rand::Rng::r#gen::<f32>(&mut *r) < p.reorder_pct {
            hold = Some((buf, dst_override));
            continue;
        }
        let (mut send_buf, send_addr) = if let Some(h) = hold.take() {
            h
        } else {
            (buf, dst_override)
        };

        // Delay + jitter
        let jitter = if p.jitter_ms > 0 {
            r.gen_range(0..=p.jitter_ms)
        } else {
            0
        };
        sleep(Duration::from_millis(p.base_delay_ms + jitter)).await;

        // Rate limit
        let len = send_buf.len();
        loop {
            let mut tbf = pipe.tbf.lock().await;
            if tbf.allow(len) {
                break;
            }
            drop(tbf);
            sleep(Duration::from_millis(1)).await;
        }

        let dst_addr = if send_addr.ip().is_unspecified() {
            default_dst
        } else {
            send_addr
        };
        let _ = send_sock.send_to(&mut send_buf, dst_addr).await;
        if dup {
            let _ = send_sock.send_to(&mut send_buf, dst_addr).await;
        }
    }
}

/// Spawn tasks for a bidirectional link.
pub async fn run_link(
    ingress_forward_port: u16,
    egress_forward_port: u16,
    rx_port: u16,
    emu: std::sync::Arc<tokio::sync::Mutex<Emulator>>,
    fwd_params: LinkParams,
    rev_params: LinkParams,
) -> Result<()> {
    // Ingress from sender
    let ingress = UdpSocket::bind(("127.0.0.1", ingress_forward_port)).await?;
    let rev_ingress = UdpSocket::bind(("127.0.0.1", egress_forward_port)).await?;

    // Egress sockets with fixed source ports
    let fwd_send = std::sync::Arc::new(UdpSocket::bind(("127.0.0.1", 0)).await?);
    let rev_send = std::sync::Arc::new(UdpSocket::bind(("127.0.0.1", 0)).await?);

    let (tx_fwd, rx_fwd) = mpsc::channel::<(Vec<u8>, SocketAddr)>(1024);
    let (tx_rev, rx_rev) = mpsc::channel::<(Vec<u8>, SocketAddr)>(1024);

    let fwd_pipe = Pipe::new(fwd_params);
    let rev_pipe = Pipe::new(rev_params);
    let rng1 = Mutex::new(StdRng::seed_from_u64(1));
    let rng2 = Mutex::new(StdRng::seed_from_u64(2));

    let rx_addr: SocketAddr = format!("127.0.0.1:{rx_port}").parse().unwrap();
    tokio::spawn(run_pipe(rx_fwd, fwd_send.clone(), rx_addr, fwd_pipe, rng1));
    tokio::spawn(run_pipe(
        rx_rev,
        rev_send.clone(),
        "127.0.0.1:9".parse().unwrap(),
        rev_pipe,
        rng2,
    ));

    // Forward ingress
    let emu_fwd = emu.clone();
    tokio::spawn(async move {
        let mut buf = vec![0u8; 65536];
        loop {
            if let Ok((n, src)) = ingress.recv_from(&mut buf).await {
                let mut e = emu_fwd.lock().await;
                e.reverse_dst.insert(ingress_forward_port, src);
                drop(e);
                let _ = tx_fwd.send((buf[..n].to_vec(), rx_addr)).await;
            }
        }
    });

    // Reverse ingress
    tokio::spawn(async move {
        let mut buf = vec![0u8; 65536];
        loop {
            if let Ok((n, _src)) = rev_ingress.recv_from(&mut buf).await {
                let dst = {
                    let e = emu.lock().await;
                    e.reverse_dst.get(&ingress_forward_port).cloned()
                };
                if let Some(dst_addr) = dst {
                    let _ = tx_rev.send((buf[..n].to_vec(), dst_addr)).await;
                }
            }
        }
    });

    Ok(())
}

/// Test scenario configurations for common RIST testing scenarios
#[derive(Clone, Debug)]
pub struct TestScenario {
    pub name: String,
    pub description: String,
    pub forward_params: LinkParams,
    pub reverse_params: LinkParams,
    pub duration_seconds: Option<u64>,
}

impl TestScenario {
    /// Dual-link bonding scenario with asymmetric quality
    pub fn bonding_asymmetric() -> Self {
        Self {
            name: "bonding_asymmetric".to_string(),
            description: "Two links with different quality characteristics for bonding tests".to_string(),
            forward_params: LinkParams::typical(),
            reverse_params: LinkParams::builder()
                .base_delay_ms(10)
                .jitter_ms(3)
                .loss_pct(0.0005)
                .rate_bps(1_000_000)
                .build(),
            duration_seconds: None,
        }
    }

    /// Degraded network scenario
    pub fn degraded_network() -> Self {
        Self {
            name: "degraded_network".to_string(),
            description: "Simulates network degradation with high loss and jitter".to_string(),
            forward_params: LinkParams::poor(),
            reverse_params: LinkParams::poor(),
            duration_seconds: Some(60),
        }
    }

    /// Good quality baseline scenario
    pub fn baseline_good() -> Self {
        Self {
            name: "baseline_good".to_string(),
            description: "Baseline good quality network for comparing against other scenarios".to_string(),
            forward_params: LinkParams::good(),
            reverse_params: LinkParams::good(),
            duration_seconds: Some(30),
        }
    }

    /// Mobile/cellular network simulation
    pub fn mobile_network() -> Self {
        Self {
            name: "mobile_network".to_string(),
            description: "Simulates mobile/cellular network characteristics".to_string(),
            forward_params: LinkParams::cellular(),
            reverse_params: LinkParams::cellular(),
            duration_seconds: Some(45),
        }
    }

    /// Varying quality scenario (starts good, degrades, recovers)
    pub fn varying_quality() -> Self {
        Self {
            name: "varying_quality".to_string(),
            description: "Network that varies in quality over time".to_string(),
            forward_params: LinkParams::typical(),
            reverse_params: LinkParams::typical(),
            duration_seconds: Some(120),
        }
    }
}

/// Network simulator orchestrator for running test scenarios
#[derive(Debug)]
pub struct NetworkOrchestrator {
    emulator: std::sync::Arc<tokio::sync::Mutex<Emulator>>,
    active_links: Vec<LinkHandle>,
    next_port_forward: u16,
    next_port_reverse: u16,
}

#[derive(Debug)]
pub struct LinkHandle {
    pub ingress_port: u16,
    pub egress_port: u16,
    pub rx_port: u16,
    pub scenario: TestScenario,
}

impl NetworkOrchestrator {
    pub fn new(seed: u64) -> Self {
        // Derive base port ranges from the seed so multiple orchestrators
        // created during tests don't allocate overlapping ports.
        // We map seed -> a small offset in the 30000-40000 range.
        let seed_offset = (seed % 1000) as u16; // 0..999
        let base_forward = 30000u16.saturating_add(seed_offset.saturating_mul(10));
        let base_reverse = 31000u16.saturating_add(seed_offset.saturating_mul(10));

        Self {
            emulator: std::sync::Arc::new(tokio::sync::Mutex::new(Emulator::new(seed))),
            active_links: Vec::new(),
            next_port_forward: base_forward,
            next_port_reverse: base_reverse,
        }
    }

    /// Start a test scenario with automatic port allocation
    pub async fn start_scenario(&mut self, scenario: TestScenario, rx_port: u16) -> Result<LinkHandle> {
        let ingress_port = self.next_port_forward;
        let egress_port = self.next_port_reverse;
        
        self.next_port_forward += 1;
        self.next_port_reverse += 1;

        run_link(
            ingress_port,
            egress_port,
            rx_port,
            self.emulator.clone(),
            scenario.forward_params.clone(),
            scenario.reverse_params.clone(),
        ).await?;

        let handle = LinkHandle {
            ingress_port,
            egress_port,
            rx_port,
            scenario,
        };

        self.active_links.push(handle.clone());
        Ok(handle)
    }

    /// Start multiple scenarios for bonding tests
    pub async fn start_bonding_scenarios(&mut self, scenarios: Vec<TestScenario>, rx_port: u16) -> Result<Vec<LinkHandle>> {
        let mut handles = Vec::new();
        for scenario in scenarios {
            let handle = self.start_scenario(scenario, rx_port).await?;
            handles.push(handle);
        }
        Ok(handles)
    }

    /// Get summary of active links
    pub fn get_active_links(&self) -> &[LinkHandle] {
        &self.active_links
    }

    /// Run a scenario for its specified duration (if any)
    pub async fn run_scenario_duration(&self, handle: &LinkHandle) -> Result<()> {
        if let Some(duration) = handle.scenario.duration_seconds {
            println!("Running scenario '{}' for {} seconds", handle.scenario.name, duration);
            sleep(Duration::from_secs(duration)).await;
            println!("Scenario '{}' completed", handle.scenario.name);
        }
        Ok(())
    }
}

impl Clone for LinkHandle {
    fn clone(&self) -> Self {
        Self {
            ingress_port: self.ingress_port,
            egress_port: self.egress_port,
            rx_port: self.rx_port,
            scenario: self.scenario.clone(),
        }
    }
}

/// Convenience function to start a typical RIST bonding test setup
pub async fn start_rist_bonding_test(rx_port: u16) -> Result<NetworkOrchestrator> {
    let mut orchestrator = NetworkOrchestrator::new(42);
    
    // Start two links with different characteristics for bonding
    let scenario_a = TestScenario::bonding_asymmetric();
    let mut scenario_b = TestScenario::bonding_asymmetric();
    
    // Modify second link to have different characteristics
    scenario_b.forward_params.base_delay_ms = 80;
    scenario_b.forward_params.loss_pct = 0.005;
    scenario_b.forward_params.rate_bps = 6_000_000;
    scenario_b.name = "bonding_asymmetric_b".to_string();
    
    let _handles = orchestrator.start_bonding_scenarios(vec![scenario_a, scenario_b], rx_port).await?;
    
    println!("RIST bonding test setup complete:");
    for (i, handle) in orchestrator.get_active_links().iter().enumerate() {
        println!("  Link {}: {} -> {} (rx: {})", 
            i + 1, handle.ingress_port, handle.egress_port, handle.rx_port);
        println!("    Scenario: {} - {}", handle.scenario.name, handle.scenario.description);
    }
    
    Ok(orchestrator)
}
