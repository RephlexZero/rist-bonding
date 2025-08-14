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
pub struct Emulator {
    pub reverse_dst: HashMap<u16, SocketAddr>,
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
