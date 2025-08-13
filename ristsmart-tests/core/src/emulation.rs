use anyhow::Result;
use ristsmart_netem::{EmulatorHandle, LinkSpec, OUParams, GEParams, DelayProfile, RateLimiter};

pub struct LinkPorts {
    /// UDP port used by both sender and receiver for RIST (must be same for both sides)
    pub port: u16,
    /// Link name for this port assignment
    pub link_name: String,
    /// IP address of the emulated namespace for this link
    pub ns_ip: std::net::Ipv4Addr,
}

pub fn build_emulator(seed: u64, links: usize) -> Result<(EmulatorHandle, Vec<LinkPorts>)> {
    // Create a runtime for async operations
    let rt = tokio::runtime::Runtime::new()?;
    
    let mut link_specs = Vec::new();
    for i in 0..links {
        let spec = LinkSpec {
            name: format!("link{i}"),
            rate_limiter: RateLimiter::Tbf, // Use the correct variant
            ou: OUParams {
                mean_bps: 10_000_000,
                tau_ms: 1000,
                sigma: 0.15,
                tick_ms: 100,
                seed: Some(seed + i as u64),
            },
            ge: GEParams {
                p_good: 0.0005,
                p_bad: 0.05,
                p: 0.01,
                r: 0.98,
                seed: Some(seed + 100 + i as u64),
            },
            delay: DelayProfile {
                delay_ms: 20,
                jitter_ms: 2,
                reorder_pct: 0.0,
            },
            ifb_ingress: false,
        };
        link_specs.push(spec);
    }
    
    let emu = rt.block_on(async {
        let emu = EmulatorHandle::new(link_specs.clone(), Some(seed)).await?;
        // Start the emulator to set up namespaces and veth pairs
        emu.start().await?;
        Ok::<_, anyhow::Error>(emu)
    })?;
    
    // Set up UDP forwarders for each link and assign ports
    let mut ports = Vec::with_capacity(links);
    for i in 0..links {
        let port = 5000 + (i * 2) as u16; // Even ports: 5000, 5002, 5004...
        let link_name = format!("link{i}");
        
        // Set up UDP forwarder from emulated namespace to receiver
        let link_handle = emu.link(&link_name).ok_or_else(|| {
            anyhow::anyhow!("Failed to get handle for link: {}", link_name)
        })?;
        rt.block_on(async {
            // Forward from emulated namespace back to localhost receiver
            link_handle.bind_forwarder(port, "127.0.0.1", port).await
        })?;
        
        // Get the namespace IP address for this link (10.i.0.2)
        let ns_ip = std::net::Ipv4Addr::new(10, i as u8, 0, 2);
        
        ports.push(LinkPorts { 
            port,
            link_name: link_name.clone(),
            ns_ip,
        });
    }
    
    Ok((emu, ports))
}

#[derive(Clone)]
pub struct EmuState {
    pub capacities_mbps: Vec<f64>,
    pub delay_ms: Vec<u64>,
    pub loss_rate: Vec<f64>,
}

pub struct EmuSnapshotter {
    links: Vec<String>,
    #[allow(dead_code)]
    runtime: tokio::runtime::Runtime,
}

impl EmuSnapshotter {
    pub fn new(_emu: &EmulatorHandle, links: usize) -> Self {
        let link_names: Vec<String> = (0..links).map(|i| format!("link{i}")).collect();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        Self { 
            links: link_names,
            runtime,
        }
    }

    pub fn snapshot(&self) -> Result<EmuState> {
        // For now, return mock data - would need actual emulator integration
        let links_count = self.links.len();
        Ok(EmuState { 
            capacities_mbps: vec![10.0; links_count],
            delay_ms: vec![20; links_count], 
            loss_rate: vec![0.001; links_count],
        })
    }

    pub fn link_bytes_since_last(&mut self) -> Result<Vec<u64>> {
        // Return mock data for now
        Ok(vec![1024; self.links.len()])
    }
}
