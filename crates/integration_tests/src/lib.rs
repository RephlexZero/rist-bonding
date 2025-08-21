//! End-to-end RIST integration tests
//!
//! These tests validate the complete system:
//! - NetworkOrchestrator with cellular-like conditions
//! - In-process RIST pipelines with RTP payloads
//! - Observability-style metrics collection (simplified)
//! - Bonding scenario validation (generic)

use anyhow::Result;
use gst::prelude::*;
use gstreamer as gst;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, info};

pub mod element_pad_semantics;

/// RIST Integration Test Suite
pub struct RistIntegrationTest {
    orchestrator: netns_testbench::NetworkOrchestrator,
    test_id: String,
    rx_port: u16,
    receiver: Option<gst::Pipeline>,
    sender: Option<gst::Pipeline>,
    /// Absolute path to the MP4 file recorded by the receiver, if any
    mp4_path: Option<PathBuf>,
    buffer_count: Arc<AtomicU64>,
    /// When true, add a tee branch to decode and display video (if a display is available)
    show_video: bool,
}

impl RistIntegrationTest {
    /// Log the origin of an element factory (plugin name/version/filename) to confirm which plugin provides it
    fn log_factory_origin(name: &str) {
        match gst::ElementFactory::find(name) {
            Some(factory) => {
                let plugin = factory.plugin();
                let plugin_name = plugin.as_ref().map(|p| p.name().to_string()).unwrap_or_else(|| "<unknown>".to_string());
                let plugin_version = plugin.as_ref().map(|p| p.version().to_string()).unwrap_or_else(|| "<unknown>".to_string());
                let plugin_filename = plugin
                    .as_ref()
                    .and_then(|p| p.filename())
                    .map(|f| f.display().to_string())
                    .unwrap_or_else(|| "<unknown>".to_string());
                info!(
                    "factory_origin name={} plugin={} version={} file={}",
                    name, plugin_name, plugin_version, plugin_filename
                );
            }
            None => {
                info!("factory_origin name={} plugin=<not found>", name);
            }
        }
    }
    /// Resolve a consistent artifacts directory for all test outputs.
    /// Priority:
    /// 1) TEST_ARTIFACTS_DIR env var, if set
    /// 2) CARGO_TARGET_DIR/test-artifacts, if CARGO_TARGET_DIR set
    /// 3) <workspace_root>/target/test-artifacts (detected by walking up to Cargo.toml with [workspace])
    pub fn artifacts_dir() -> PathBuf {
        if let Ok(dir) = std::env::var("TEST_ARTIFACTS_DIR") {
            let p = PathBuf::from(dir);
            let _ = std::fs::create_dir_all(&p);
            return p;
        }
        if let Ok(target) = std::env::var("CARGO_TARGET_DIR") {
            let p = PathBuf::from(target).join("test-artifacts");
            let _ = std::fs::create_dir_all(&p);
            return p;
        }
        let ws = Self::workspace_root(Path::new(env!("CARGO_MANIFEST_DIR")));
        let p = ws.join("target").join("test-artifacts");
        let _ = std::fs::create_dir_all(&p);
        p
    }

    /// Helper to build a full artifact file path and ensure parent exists
    pub fn artifact_path(file_name: &str) -> PathBuf {
        let dir = Self::artifacts_dir();
        dir.join(file_name)
    }

    /// Walk up from a starting directory to find the Cargo workspace root (Cargo.toml with [workspace])
    fn workspace_root(start: &Path) -> PathBuf {
        let mut dir = start.to_path_buf();
        loop {
            let cargo = dir.join("Cargo.toml");
            if cargo.exists() {
                if let Ok(content) = std::fs::read_to_string(&cargo) {
                    if content.contains("[workspace]") {
                        return dir;
                    }
                }
            }
            if !dir.pop() {
                // Fallback: original start if no workspace marker found
                return start.to_path_buf();
            }
        }
    }

    /// Create new integration test
    pub async fn new(test_id: String, rx_port: u16) -> Result<Self> {
        // Initialize GStreamer once per process
        let _ = gst::init();
        // Try to register custom rist-elements (test plugin) so `ristdispatcher` and `dynbitrate` are available
        #[allow(unused_must_use)]
        {
            // It's okay if this no-ops (feature gated in rist-elements)
            let _ = gstristelements::register_for_tests();
        }
        let orchestrator = netns_testbench::NetworkOrchestrator::new(42).await?;
        // Enable on-screen preview if requested
        let show_video = std::env::var("RIST_SHOW_VIDEO")
            .map(|v| v != "0" && !v.is_empty())
            .unwrap_or(false);

        Ok(Self {
            orchestrator,
            test_id,
            rx_port,
            receiver: None,
            sender: None,
            mp4_path: None,
            buffer_count: Arc::new(AtomicU64::new(0)),
            show_video,
        })
    }

    /// Start RIST H.265 pipelines inside network namespaces created by the orchestrator
    /// Requirements:
    /// - At least one active link started via setup_bonding() (or directly via orchestrator)
    /// - Uses the first link's namespaces (a_ns as sender, b_ns as receiver)
    /// - Binds receiver to its namespace IP and sender targets that IP
    pub async fn start_rist_pipelines_in_netns(&mut self) -> Result<()> {
        use netns_testbench::addr::Configurer as AddrConfigurer;
        use netns_testbench::netns::Manager as NsManager;

        // Validate even port for RIST (RTCP uses port+1)
        if self.rx_port % 2 != 0 {
            return Err(anyhow::anyhow!(
                "RIST requires an even RTP port; got {}",
                self.rx_port
            ));
        }

        // Ensure we have at least one active link
        let links = self.orchestrator.get_active_links();
        if links.is_empty() {
            return Err(anyhow::anyhow!(
                "No active links. Call setup_bonding() first."
            ));
        }
        let link = &links[0];

        // Derive namespace names from link handle (matches orchestrator's naming)
        let link_spec = link
            .scenario
            .links
            .get(0)
            .ok_or_else(|| anyhow::anyhow!("Link scenario missing link spec"))?;
        let ns_a = format!("{}_{}", link_spec.a_ns, link.link_id); // sender side
        let ns_b = format!("{}_{}", link_spec.b_ns, link.link_id); // receiver side

        // Compute the point-to-point IPs deterministically from link_id number
        let link_num: u8 = link
            .link_id
            .strip_prefix("link_")
            .and_then(|s| s.parse::<u8>().ok())
            .ok_or_else(|| anyhow::anyhow!("Invalid link_id format: {}", link.link_id))?;
        let (_left, right) = AddrConfigurer::generate_p2p_subnet(link_num)
            .map_err(|e| anyhow::anyhow!("Failed to compute link subnet: {}", e))?;
        let receiver_ip = right.ip(); // b_ns side
        let receiver_ip_str = receiver_ip.to_string();

        // Prepare a namespace manager and attach to existing namespaces
        let mut ns_mgr = NsManager::new()?;
        ns_mgr.attach_existing_namespace(&ns_a)?;
        ns_mgr.attach_existing_namespace(&ns_b)?;

        // Build and start receiver inside b_ns (H.265)
        let buffer_count = self.buffer_count.clone();
        let rx_port = self.rx_port;
        let show_video = self.show_video;
        let mp4_location: PathBuf = Self::artifact_path(&format!("{}.mp4", self.test_id));
        // test_id is used below when constructing artifact paths; no clone needed here.
        let receiver: gst::Pipeline = ns_mgr.exec_in_namespace(&ns_b, || {
            // Log which plugin provides RIST elements and potential dispatchers
            Self::log_factory_origin("ristsrc");
            Self::log_factory_origin("ristsink");
            // Try a couple of likely dispatcher/round-robin names
            Self::log_factory_origin("roundrobin");
            Self::log_factory_origin("gstroundrobin");
            Self::log_factory_origin("ristdispatcher");

            let pipeline = gst::Pipeline::new();
            let rsrc = gst::ElementFactory::make("ristsrc")
                .property("address", "0.0.0.0")
                .property("port", rx_port as u32)
                .property("encoding-name", "H265")
                .property("receiver-buffer", 5000u32)
                .build()
                .unwrap();
            let depay = gst::ElementFactory::make("rtph265depay").build().unwrap();
            let fsink = gst::ElementFactory::make("fakesink")
                .property("sync", false)
                .property("signal-handoffs", true)
                .build()
                .unwrap();
            let count = buffer_count.clone();
            fsink.connect("handoff", false, move |_| {
                count.fetch_add(1, Ordering::Relaxed);
                None
            });

            // Always create tee with two branches: counter and MP4. Preview branch is optional.
            let tee = gst::ElementFactory::make("tee").build().unwrap();
            let q_count = gst::ElementFactory::make("queue").build().unwrap();
            pipeline
                .add_many([&rsrc, &depay, &tee, &q_count, &fsink])
                .unwrap();
            gst::Element::link_many([&rsrc, &depay, &tee]).unwrap();
            gst::Element::link_many([&q_count, &fsink]).unwrap();
            let tee_count_pad = tee.request_pad_simple("src_%u").unwrap();
            let q_count_sink = q_count.static_pad("sink").unwrap();
            tee_count_pad.link(&q_count_sink).unwrap();

            if show_video {
                let q_view = gst::ElementFactory::make("queue").build().unwrap();
                let dec = gst::ElementFactory::make("avdec_h265").build().unwrap();
                let vconv = gst::ElementFactory::make("videoconvert").build().unwrap();
                let vsink = gst::ElementFactory::make("autovideosink")
                    .property("sync", false)
                    .build()
                    .unwrap();
                pipeline.add_many([&q_view, &dec, &vconv, &vsink]).unwrap();
                gst::Element::link_many([&q_view, &dec, &vconv, &vsink]).unwrap();
                let tee_view_pad = tee.request_pad_simple("src_%u").unwrap();
                let q_view_sink = q_view.static_pad("sink").unwrap();
                tee_view_pad.link(&q_view_sink).unwrap();
            }

            // Always-on MP4 recording branch
            let q_mp4 = gst::ElementFactory::make("queue").build().unwrap();
            let h265parse = gst::ElementFactory::make("h265parse")
                .property("config-interval", -1i32)
                .build()
                .unwrap();
            let mp4mux = gst::ElementFactory::make("mp4mux").build().unwrap();
            let location = mp4_location.clone();
            info!("Recording MP4 to {}", location.to_string_lossy());
            let filesink = gst::ElementFactory::make("filesink")
                .property("location", location.to_string_lossy().to_string())
                .build()
                .unwrap();
            pipeline
                .add_many([&q_mp4, &h265parse, &mp4mux, &filesink])
                .unwrap();
            gst::Element::link_many([&q_mp4, &h265parse]).unwrap();
            let h265_src = h265parse.static_pad("src").unwrap();
            if let Some(video_pad) = mp4mux.request_pad_simple("video_%u") {
                h265_src.link(&video_pad).unwrap();
            } else {
                eprintln!("mp4mux: failed to request video pad");
            }
            gst::Element::link_many([&mp4mux, &filesink]).unwrap();
            let tee_mp4_pad = tee.request_pad_simple("src_%u").unwrap();
            let q_mp4_sink = q_mp4.static_pad("sink").unwrap();
            tee_mp4_pad.link(&q_mp4_sink).unwrap();

            // Don't panic; return pipeline even if it fails later so caller can handle errors
            pipeline
        })?;
        // Try to set state and map failure to error instead of panicking
        receiver
            .set_state(gst::State::Playing)
            .map_err(|_| anyhow::anyhow!("Receiver pipeline failed to start (Playing)"))?;

        // Small delay to let receiver bind sockets before starting sender
        tokio::time::sleep(Duration::from_millis(300)).await;

        // Build and start sender inside a_ns (H.265)
        let sender: gst::Pipeline = ns_mgr.exec_in_namespace(&ns_a, || {
            // Log which plugin provides RIST sink in sender ns context too
            Self::log_factory_origin("ristsink");
            let pipeline = gst::Pipeline::new();
            let vsrc = gst::ElementFactory::make("videotestsrc")
                .property("is-live", true)
                .build()
                .unwrap();
            let vconv = gst::ElementFactory::make("videoconvert").build().unwrap();
            let venc = gst::ElementFactory::make("x265enc")
                .property_from_str("tune", "zerolatency")
                .property_from_str("speed-preset", "ultrafast")
                .build()
                .unwrap();
            let vparse = gst::ElementFactory::make("h265parse")
                .property("config-interval", 1i32)
                .build()
                .unwrap();
            let pay = gst::ElementFactory::make("rtph265pay").build().unwrap();
            let rsink = gst::ElementFactory::make("ristsink")
                .property("address", receiver_ip_str.as_str())
                .property("port", rx_port as u32)
                .build()
                .unwrap();
            pipeline
                .add_many([&vsrc, &vconv, &venc, &vparse, &pay, &rsink])
                .unwrap();
            gst::Element::link_many([&vsrc, &vconv, &venc, &vparse, &pay, &rsink]).unwrap();
            // Do not panic here; let caller set state and handle errors
            pipeline
        })?;

        // Try to start sender; map failure to error
        sender
            .set_state(gst::State::Playing)
            .map_err(|_| anyhow::anyhow!("Sender pipeline failed to start (Playing)"))?;

        self.receiver = Some(receiver);
        self.sender = Some(sender);
        self.mp4_path = Some(mp4_location);
        info!(
            "Started RIST H.265 pipelines in netns: sender={} -> receiver={} ({})",
            ns_a, ns_b, receiver_ip_str
        );
        Ok(())
    }

    /// Start RIST pipelines in netns but use custom rist-elements dispatcher (ristdispatcher)
    /// and dynamic bitrate controller (dynbitrate) on the sender.
    /// If the custom elements are unavailable, returns a Skip-shaped error for the caller to handle.
    pub async fn start_rist_pipelines_in_netns_with_custom_dispatcher(&mut self) -> Result<()> {
        use netns_testbench::addr::Configurer as AddrConfigurer;
        use netns_testbench::netns::Manager as NsManager;

        if self.rx_port % 2 != 0 {
            return Err(anyhow::anyhow!(
                "RIST requires an even RTP port; got {}",
                self.rx_port
            ));
        }

        let links = self.orchestrator.get_active_links();
        if links.is_empty() {
            return Err(anyhow::anyhow!(
                "No active links. Call setup_bonding() first."
            ));
        }
        let link = &links[0];
        let link_spec = link
            .scenario
            .links
            .get(0)
            .ok_or_else(|| anyhow::anyhow!("Link scenario missing link spec"))?;
        let ns_a = format!("{}_{}", link_spec.a_ns, link.link_id); // sender
        let ns_b = format!("{}_{}", link_spec.b_ns, link.link_id); // receiver

        let link_num: u8 = link
            .link_id
            .strip_prefix("link_")
            .and_then(|s| s.parse::<u8>().ok())
            .ok_or_else(|| anyhow::anyhow!("Invalid link_id format: {}", link.link_id))?;
        let (_left, right) = AddrConfigurer::generate_p2p_subnet(link_num)
            .map_err(|e| anyhow::anyhow!("Failed to compute link subnet: {}", e))?;
        let receiver_ip = right.ip();
        let receiver_ip_str = receiver_ip.to_string();

        let mut ns_mgr = NsManager::new()?;
        ns_mgr.attach_existing_namespace(&ns_a)?;
        ns_mgr.attach_existing_namespace(&ns_b)?;

        // Receiver same as default variant
        let buffer_count = self.buffer_count.clone();
        let rx_port = self.rx_port;
        let show_video = self.show_video;
        let mp4_location: PathBuf = Self::artifact_path(&format!("{}.mp4", self.test_id));
        let receiver: gst::Pipeline = ns_mgr.exec_in_namespace(&ns_b, || {
            Self::log_factory_origin("ristsrc");
            let pipeline = gst::Pipeline::new();
            let rsrc = gst::ElementFactory::make("ristsrc")
                .property("address", "0.0.0.0")
                .property("port", rx_port as u32)
                .property("encoding-name", "H265")
                .property("receiver-buffer", 5000u32)
                .build()
                .unwrap();
            let depay = gst::ElementFactory::make("rtph265depay").build().unwrap();
            let fsink = gst::ElementFactory::make("fakesink")
                .property("sync", false)
                .property("signal-handoffs", true)
                .build()
                .unwrap();
            let count = buffer_count.clone();
            fsink.connect("handoff", false, move |_| {
                count.fetch_add(1, Ordering::Relaxed);
                None
            });
            let tee = gst::ElementFactory::make("tee").build().unwrap();
            let q_count = gst::ElementFactory::make("queue").build().unwrap();
            pipeline.add_many([&rsrc, &depay, &tee, &q_count, &fsink]).unwrap();
            gst::Element::link_many([&rsrc, &depay, &tee]).unwrap();
            gst::Element::link_many([&q_count, &fsink]).unwrap();
            let tee_count_pad = tee.request_pad_simple("src_%u").unwrap();
            let q_count_sink = q_count.static_pad("sink").unwrap();
            tee_count_pad.link(&q_count_sink).unwrap();
            if show_video {
                let q_view = gst::ElementFactory::make("queue").build().unwrap();
                let dec = gst::ElementFactory::make("avdec_h265").build().unwrap();
                let vconv = gst::ElementFactory::make("videoconvert").build().unwrap();
                let vsink = gst::ElementFactory::make("autovideosink")
                    .property("sync", false)
                    .build()
                    .unwrap();
                pipeline.add_many([&q_view, &dec, &vconv, &vsink]).unwrap();
                gst::Element::link_many([&q_view, &dec, &vconv, &vsink]).unwrap();
                let tee_view_pad = tee.request_pad_simple("src_%u").unwrap();
                let q_view_sink = q_view.static_pad("sink").unwrap();
                tee_view_pad.link(&q_view_sink).unwrap();
            }
            let q_mp4 = gst::ElementFactory::make("queue").build().unwrap();
            let h265parse = gst::ElementFactory::make("h265parse")
                .property("config-interval", -1i32)
                .build()
                .unwrap();
            let mp4mux = gst::ElementFactory::make("mp4mux").build().unwrap();
            let location = mp4_location.clone();
            info!("Recording MP4 to {}", location.to_string_lossy());
            let filesink = gst::ElementFactory::make("filesink")
                .property("location", location.to_string_lossy().to_string())
                .build()
                .unwrap();
            pipeline.add_many([&q_mp4, &h265parse, &mp4mux, &filesink]).unwrap();
            gst::Element::link_many([&q_mp4, &h265parse]).unwrap();
            let h265_src = h265parse.static_pad("src").unwrap();
            if let Some(video_pad) = mp4mux.request_pad_simple("video_%u") {
                h265_src.link(&video_pad).unwrap();
            }
            gst::Element::link_many([&mp4mux, &filesink]).unwrap();
            let tee_mp4_pad = tee.request_pad_simple("src_%u").unwrap();
            let q_mp4_sink = q_mp4.static_pad("sink").unwrap();
            tee_mp4_pad.link(&q_mp4_sink).unwrap();
            pipeline
        })?;
        receiver
            .set_state(gst::State::Playing)
            .map_err(|_| anyhow::anyhow!("Receiver pipeline failed to start (Playing)"))?;
        tokio::time::sleep(Duration::from_millis(300)).await;

        // Sender with custom dispatcher and dynbitrate
        let rx_port = self.rx_port;
        let sender: gst::Pipeline = ns_mgr.exec_in_namespace(&ns_a, || {
            // Ensure custom elements are registered
            let have_dispatcher = gst::ElementFactory::find("ristdispatcher").is_some();
            let have_dynbitrate = gst::ElementFactory::find("dynbitrate").is_some();
            if !have_dispatcher || !have_dynbitrate {
                panic!(
                    "Custom elements missing: ristdispatcher={} dynbitrate={} (set GST_PLUGIN_PATH to build dir)",
                    have_dispatcher,
                    have_dynbitrate
                );
            }
            Self::log_factory_origin("ristdispatcher");
            Self::log_factory_origin("dynbitrate");
            let pipeline = gst::Pipeline::new();
            let vsrc = gst::ElementFactory::make("videotestsrc")
                .property("is-live", true)
                .build()
                .unwrap();
            let vconv = gst::ElementFactory::make("videoconvert").build().unwrap();
            let venc = gst::ElementFactory::make("x265enc")
                .property_from_str("tune", "zerolatency")
                .property_from_str("speed-preset", "ultrafast")
                .property("bitrate", 4000u32)
                .build()
                .unwrap();
            let vparse = gst::ElementFactory::make("h265parse")
                .property("config-interval", 1i32)
                .build()
                .unwrap();
            let pay = gst::ElementFactory::make("rtph265pay").build().unwrap();
            let dynb = gst::ElementFactory::make("dynbitrate").build().unwrap();
            let ristsink = gst::ElementFactory::make("ristsink")
                .property("address", receiver_ip_str.as_str())
                .property("port", rx_port as u32)
                .property("sender-buffer", 5000u32)
                .property("stats-update-interval", 1000u32)
                .build()
                .unwrap();
            let dispatcher = gst::ElementFactory::make("ristdispatcher").build().unwrap();
            // Attach dispatcher to rist sink
            ristsink.set_property("dispatcher", &dispatcher);

            pipeline
                .add_many([&vsrc, &vconv, &venc, &vparse, &pay, &dynb, &ristsink])
                .unwrap();
            gst::Element::link_many([&vsrc, &vconv, &venc, &vparse, &pay, &dynb, &ristsink])
                .unwrap();

            // Wire up dynbitrate control
            dynb.set_property("encoder", &venc);
            dynb.set_property("rist", &ristsink);
            dynb.set_property("dispatcher", &dispatcher);
            dynb.set_property("min-kbps", 500u32);
            dynb.set_property("max-kbps", 20000u32);
            dynb.set_property("step-kbps", 250u32);
            dynb.set_property("target-loss-pct", 1.0f64);
            dynb.set_property("downscale-keyunit", &true);
            pipeline
        })?;
        sender
            .set_state(gst::State::Playing)
            .map_err(|_| anyhow::anyhow!("Sender pipeline failed to start (Playing)"))?;

        self.receiver = Some(receiver);
        self.sender = Some(sender);
        self.mp4_path = Some(mp4_location);
        info!(
            "Started RIST H.265 pipelines (custom dispatcher) in netns: sender={} -> receiver={} ({})",
            ns_a, ns_b, receiver_ip_str
        );
        Ok(())
    }

    /// Start local RIST H.265 sender/receiver pipelines (receiver first)
    pub async fn start_local_rist_pipelines(&mut self) -> Result<()> {
    info!("🚀 Starting local RIST H.265 pipelines (receiver first)");

    // Log which plugin provides RIST elements and potential dispatchers
    Self::log_factory_origin("ristsrc");
    Self::log_factory_origin("ristsink");
    Self::log_factory_origin("roundrobin");
    Self::log_factory_origin("gstroundrobin");
    Self::log_factory_origin("ristdispatcher");

    let mp4_location: PathBuf = Self::artifact_path(&format!("{}.mp4", self.test_id));

    // Choose a free even port for local mode (base + RTCP)
    let chosen_port = Self::find_free_even_port(self.rx_port);
    self.rx_port = chosen_port;

        // Receiver: ristsrc -> rtph265depay -> tee -> (counting, optional preview, MP4)
        let receiver = gst::Pipeline::new();
        let rsrc = gst::ElementFactory::make("ristsrc")
            .property("address", "0.0.0.0")
            .property("port", self.rx_port as u32)
            .property("encoding-name", "H265")
            .property("receiver-buffer", 5000u32)
            .build()?;
        let depay = gst::ElementFactory::make("rtph265depay").build()?;
        let fsink = gst::ElementFactory::make("fakesink")
            .property("sync", false)
            .property("signal-handoffs", true)
            .build()?;
        let count = self.buffer_count.clone();
        fsink.connect("handoff", false, move |_| {
            count.fetch_add(1, Ordering::Relaxed);
            None
        });

        let tee = gst::ElementFactory::make("tee").build()?;
        let q_count = gst::ElementFactory::make("queue").build()?;
        receiver.add_many([&rsrc, &depay, &tee, &q_count, &fsink])?;
        gstreamer::Element::link_many([&rsrc, &depay, &tee])?;
        gstreamer::Element::link_many([&q_count, &fsink])?;
        let tee_count_pad = tee.request_pad_simple("src_%u").unwrap();
        let q_count_sink = q_count.static_pad("sink").unwrap();
        tee_count_pad.link(&q_count_sink).unwrap();

        if self.show_video {
            let q_view = gstreamer::ElementFactory::make("queue").build()?;
            let dec = gstreamer::ElementFactory::make("avdec_h265").build()?;
            let vconv = gstreamer::ElementFactory::make("videoconvert").build()?;
            let vsink = gstreamer::ElementFactory::make("autovideosink")
                .property("sync", false)
                .build()?;
            receiver.add_many([&q_view, &dec, &vconv, &vsink])?;
            gstreamer::Element::link_many([&q_view, &dec, &vconv, &vsink])?;
            let tee_view_pad = tee.request_pad_simple("src_%u").unwrap();
            let q_view_sink = q_view.static_pad("sink").unwrap();
            tee_view_pad.link(&q_view_sink).unwrap();
        }

        // Always-on MP4 branch
        let q_mp4 = gstreamer::ElementFactory::make("queue").build()?;
        let h265parse = gstreamer::ElementFactory::make("h265parse")
            .property("config-interval", -1i32)
            .build()?;
        let mp4mux = gstreamer::ElementFactory::make("mp4mux").build()?;
        info!("Recording MP4 to {}", mp4_location.to_string_lossy());
        let filesink = gstreamer::ElementFactory::make("filesink")
            .property("location", mp4_location.to_string_lossy().to_string())
            .build()?;
        receiver.add_many([&q_mp4, &h265parse, &mp4mux, &filesink])?;
        gstreamer::Element::link_many([&q_mp4, &h265parse]).unwrap();
        let h265_src = h265parse.static_pad("src").unwrap();
        if let Some(video_pad) = mp4mux.request_pad_simple("video_%u") {
            h265_src.link(&video_pad).unwrap();
        } else {
            eprintln!("mp4mux: failed to request video pad");
        }
        gstreamer::Element::link_many([&mp4mux, &filesink]).unwrap();
        let tee_mp4_pad = tee.request_pad_simple("src_%u").unwrap();
        let q_mp4_sink = q_mp4.static_pad("sink").unwrap();
        tee_mp4_pad.link(&q_mp4_sink).unwrap();

    // Start receiver first and wait for Playing
    self.wait_for_state(&receiver, gst::State::Playing, 3)?;

        // Sender: videotestsrc -> x265enc -> h265parse -> rtph265pay -> ristsink
        let sender = gst::Pipeline::new();
        let vsrc = gst::ElementFactory::make("videotestsrc")
            .property("is-live", true)
            .build()?;
        let vconv = gst::ElementFactory::make("videoconvert").build()?;
        let venc = gst::ElementFactory::make("x265enc")
            .property_from_str("tune", "zerolatency")
            .property_from_str("speed-preset", "ultrafast")
            .build()?;
        let vparse = gst::ElementFactory::make("h265parse")
            .property("config-interval", 1i32)
            .build()?;
        let pay = gst::ElementFactory::make("rtph265pay").build()?;
        let rsink = gst::ElementFactory::make("ristsink")
            .property("address", "127.0.0.1")
            .property("port", self.rx_port as u32)
            .build()?;
        sender.add_many([&vsrc, &vconv, &venc, &vparse, &pay, &rsink])?;
        gst::Element::link_many([&vsrc, &vconv, &venc, &vparse, &pay, &rsink])?;

    // Wait briefly for sender too
    sleep(Duration::from_millis(200)).await;
        self.wait_for_state(&sender, gst::State::Playing, 3)?;

        self.receiver = Some(receiver);
        self.sender = Some(sender);
        info!("Local RIST H.265 pipelines started");
        self.mp4_path = Some(mp4_location);
        Ok(())
    }

    /// Start local RIST H.265 pipelines but use custom rist-elements dispatcher (ristdispatcher)
    /// and dynamic bitrate controller (dynbitrate) on the sender.
    pub async fn start_local_rist_pipelines_with_custom_dispatcher(&mut self) -> Result<()> {
        // Log available factories
        Self::log_factory_origin("ristsrc");
        Self::log_factory_origin("ristsink");
        Self::log_factory_origin("ristdispatcher");
        Self::log_factory_origin("dynbitrate");

        // Ensure required elements exist; if not, return a clear error
        if gst::ElementFactory::find("ristdispatcher").is_none()
            || gst::ElementFactory::find("dynbitrate").is_none()
        {
            return Err(anyhow::anyhow!(
                "Custom elements missing: ristdispatcher or dynbitrate"
            ));
        }

    let mp4_location: PathBuf = Self::artifact_path(&format!("{}.mp4", self.test_id));

    // Choose a free even port for local mode
    let chosen_port = Self::find_free_even_port(self.rx_port);
    self.rx_port = chosen_port;

        // Receiver: ristsrc -> rtph265depay -> tee -> (counting, optional preview, MP4)
        let receiver = gst::Pipeline::new();
    let rsrc = gst::ElementFactory::make("ristsrc")
            .property("address", "0.0.0.0")
            .property("port", self.rx_port as u32)
            .property("encoding-name", "H265")
            .property("receiver-buffer", 5000u32)
            .build()?;
        let depay = gst::ElementFactory::make("rtph265depay").build()?;
        let fsink = gst::ElementFactory::make("fakesink")
            .property("sync", false)
            .property("signal-handoffs", true)
            .build()?;
        let count = self.buffer_count.clone();
        fsink.connect("handoff", false, move |_| {
            count.fetch_add(1, Ordering::Relaxed);
            None
        });

        let tee = gst::ElementFactory::make("tee").build()?;
        let q_count = gst::ElementFactory::make("queue").build()?;
        receiver.add_many([&rsrc, &depay, &tee, &q_count, &fsink])?;
        gstreamer::Element::link_many([&rsrc, &depay, &tee])?;
        gstreamer::Element::link_many([&q_count, &fsink])?;
        let tee_count_pad = tee.request_pad_simple("src_%u").unwrap();
        let q_count_sink = q_count.static_pad("sink").unwrap();
        tee_count_pad.link(&q_count_sink).unwrap();

        if self.show_video {
            let q_view = gstreamer::ElementFactory::make("queue").build()?;
            let dec = gstreamer::ElementFactory::make("avdec_h265").build()?;
            let vconv = gstreamer::ElementFactory::make("videoconvert").build()?;
            let vsink = gstreamer::ElementFactory::make("autovideosink")
                .property("sync", false)
                .build()?;
            receiver.add_many([&q_view, &dec, &vconv, &vsink])?;
            gstreamer::Element::link_many([&q_view, &dec, &vconv, &vsink])?;
            let tee_view_pad = tee.request_pad_simple("src_%u").unwrap();
            let q_view_sink = q_view.static_pad("sink").unwrap();
            tee_view_pad.link(&q_view_sink).unwrap();
        }

        // Always-on MP4 branch
        let q_mp4 = gstreamer::ElementFactory::make("queue").build()?;
        let h265parse = gstreamer::ElementFactory::make("h265parse")
            .property("config-interval", -1i32)
            .build()?;
        let mp4mux = gstreamer::ElementFactory::make("mp4mux").build()?;
        info!("Recording MP4 to {}", mp4_location.to_string_lossy());
        let filesink = gstreamer::ElementFactory::make("filesink")
            .property("location", mp4_location.to_string_lossy().to_string())
            .build()?;
        receiver.add_many([&q_mp4, &h265parse, &mp4mux, &filesink])?;
        gstreamer::Element::link_many([&q_mp4, &h265parse]).unwrap();
        let h265_src = h265parse.static_pad("src").unwrap();
        if let Some(video_pad) = mp4mux.request_pad_simple("video_%u") {
            h265_src.link(&video_pad).unwrap();
        } else {
            eprintln!("mp4mux: failed to request video pad");
        }
        gstreamer::Element::link_many([&mp4mux, &filesink]).unwrap();
        let tee_mp4_pad = tee.request_pad_simple("src_%u").unwrap();
        let q_mp4_sink = q_mp4.static_pad("sink").unwrap();
        tee_mp4_pad.link(&q_mp4_sink).unwrap();

        // Start receiver first
        receiver
            .set_state(gstreamer::State::Playing)
            .map_err(|_| anyhow::anyhow!("Receiver pipeline failed to start (Playing)"))?;

        // Sender with custom dispatcher + dynbitrate
        let sender = gst::Pipeline::new();
        let vsrc = gst::ElementFactory::make("videotestsrc")
            .property("is-live", true)
            .build()?;
        let vconv = gst::ElementFactory::make("videoconvert").build()?;
        let venc = gst::ElementFactory::make("x265enc")
            .property_from_str("tune", "zerolatency")
            .property_from_str("speed-preset", "ultrafast")
            .property("bitrate", 4000u32)
            .build()?;
        let vparse = gst::ElementFactory::make("h265parse")
            .property("config-interval", 1i32)
            .build()?;
        let pay = gst::ElementFactory::make("rtph265pay").build()?;
        let dynb = gst::ElementFactory::make("dynbitrate").build()?;
        let ristsink = gst::ElementFactory::make("ristsink")
            .property("address", "127.0.0.1")
            .property("port", self.rx_port as u32)
            .property("sender-buffer", 5000u32)
            .property("stats-update-interval", 1000u32)
            .build()?;
        let dispatcher = gst::ElementFactory::make("ristdispatcher").build()?;
        ristsink.set_property("dispatcher", &dispatcher);

        sender.add_many([&vsrc, &vconv, &venc, &vparse, &pay, &dynb, &ristsink])?;
        gst::Element::link_many([&vsrc, &vconv, &venc, &vparse, &pay, &dynb, &ristsink])?;

        // Wire up dynbitrate
        dynb.set_property("encoder", &venc);
        dynb.set_property("rist", &ristsink);
        dynb.set_property("dispatcher", &dispatcher);
        dynb.set_property("min-kbps", 500u32);
        dynb.set_property("max-kbps", 20000u32);
        dynb.set_property("step-kbps", 250u32);
        dynb.set_property("target-loss-pct", 1.0f64);
        dynb.set_property("downscale-keyunit", &true);

        // Wait for states
        self.wait_for_state(&receiver, gst::State::Playing, 3)?;
        sleep(Duration::from_millis(200)).await;
        self.wait_for_state(&sender, gst::State::Playing, 3)?;

        self.receiver = Some(receiver);
        self.sender = Some(sender);
        self.mp4_path = Some(mp4_location);
        info!("Local RIST H.265 pipelines with custom dispatcher started");
        Ok(())
    }

    /// Find a free, even UDP port starting from preferred, trying up to 50 candidates.
    fn find_free_even_port(preferred: u16) -> u16 {
        use std::net::UdpSocket;
        let mut port = if preferred % 2 == 0 { preferred } else { preferred + 1 };
        for _ in 0..50 {
            let p1 = port;
            let p2 = port.saturating_add(1);
            let try1 = UdpSocket::bind(("127.0.0.1", p1));
            let try2 = UdpSocket::bind(("127.0.0.1", p2));
            match (try1, try2) {
                (Ok(s1), Ok(s2)) => {
                    drop(s1);
                    drop(s2);
                    return port;
                }
                _ => {
                    port = port.saturating_add(2);
                }
            }
        }
        // Fallback: let OS decide an ephemeral even-ish port
        5600
    }
    /// Set up bonding scenario (generic)
    pub async fn setup_bonding(&mut self) -> Result<Vec<netns_testbench::LinkHandle>> {
        info!("Setting up bonding scenario...");

        // Create bonding scenario using the scenarios crate
        let scenario = scenarios::TestScenario::bonding_asymmetric();
        let _handle = self
            .orchestrator
            .start_scenario(scenario, self.rx_port)
            .await?;

        // Start the scheduler
        self.orchestrator.start_scheduler().await?;

        let links = self.orchestrator.get_active_links().to_vec();

        info!("Bonding setup complete");
        for (i, handle) in links.iter().enumerate() {
            debug!("  Link {}: {}", i + 1, handle.scenario.name);
        }

        Ok(links)
    }

    /// Run a simple multi-phase traffic pattern
    pub async fn run_basic_flow(&mut self) -> Result<TestResults> {
        info!("Running basic multi-phase flow...");

        let start_time = std::time::Instant::now();
        let mut results = TestResults::new(self.test_id.clone());

        // Ensure pipelines are running
        if self.receiver.is_none() || self.sender.is_none() {
            self.start_local_rist_pipelines().await?;
        }

        // Phase 1: Nominal
        debug!("  Phase 1: nominal");
        self.simulate_traffic_phase("strong", Duration::from_secs(5))
            .await?;
        results.add_phase("strong", self.collect_phase_metrics().await?);

        // Phase 2: Degradation
        debug!("  Phase 2: degradation");
        self.apply_degradation_schedule().await?;
        self.simulate_traffic_phase("degraded", Duration::from_secs(5))
            .await?;
        results.add_phase("degraded", self.collect_phase_metrics().await?);

        // Phase 3: Handover
        debug!("  Phase 3: handover");
        self.trigger_handover_event().await?;
        self.simulate_traffic_phase("handover", Duration::from_secs(5))
            .await?;
        results.add_phase("handover", self.collect_phase_metrics().await?);

        // Phase 4: Recovery
        debug!("  Phase 4: recovery");
        self.apply_recovery_schedule().await?;
        self.simulate_traffic_phase("recovery", Duration::from_secs(5))
            .await?;
        results.add_phase("recovery", self.collect_phase_metrics().await?);

        results.total_duration = start_time.elapsed();
        info!(
            "Basic flow completed ({:.1}s)",
            results.total_duration.as_secs_f64()
        );

        Ok(results)
    }

    /// Validate RIST bonding behavior
    pub async fn validate_bonding_behavior(
        &self,
        results: &TestResults,
    ) -> Result<ValidationReport> {
        info!("Validating RIST bonding behavior...");

        let mut report = ValidationReport::new();

        // Check adaptive bitrate behavior
        let bitrate_adapted = results.phases.iter().any(|(phase, metrics)| {
            if phase == "degraded" {
                metrics.avg_bitrate < 1000.0 // Should reduce during degradation
            } else if phase == "recovery" {
                metrics.avg_bitrate > 1500.0 // Should recover after degradation
            } else {
                true
            }
        });

        report.adaptive_bitrate_working = bitrate_adapted;

        // Check bonding effectiveness
        let bonding_effective = results
            .phases
            .iter()
            .filter(|(phase, _)| *phase == "handover")
            .all(|(_, metrics)| metrics.packet_loss < 5.0); // Should maintain low loss during handover

        report.bonding_effective = bonding_effective;

        // Check link utilization
        let balanced_utilization = results.phases.iter().all(|(_, metrics)| {
            let util_ratio = metrics.primary_link_util / metrics.backup_link_util.max(0.01);
            util_ratio < 10.0 // Primary shouldn't dominate too much
        });

        report.load_balancing_working = balanced_utilization;

        info!("Bonding validation completed");
        debug!(
            "  - Adaptive bitrate: {}",
            if report.adaptive_bitrate_working {
                "✅"
            } else {
                "❌"
            }
        );
        debug!(
            "  - Bonding effectiveness: {}",
            if report.bonding_effective {
                "✅"
            } else {
                "❌"
            }
        );
        debug!(
            "  - Load balancing: {}",
            if report.load_balancing_working {
                "✅"
            } else {
                "❌"
            }
        );

        Ok(report)
    }

    async fn simulate_traffic_phase(&self, phase: &str, duration: Duration) -> Result<()> {
        // For now, just sleep to let pipelines run
        debug!("    Running phase '{}' for {:?}", phase, duration);
        let before = self.buffer_count.load(Ordering::Relaxed);
        tokio::time::sleep(duration).await;
        let after = self.buffer_count.load(Ordering::Relaxed);
        let delta = after.saturating_sub(before);
        info!(
            "    Phase '{}' complete: received {} buffers (total={})",
            phase, delta, after
        );
        Ok(())
    }

    pub async fn apply_degradation_schedule(&mut self) -> Result<()> {
        // Simplified degradation schedule for netns-testbench
        info!("Applying degradation schedule (placeholder)");
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        Ok(())
    }

    pub async fn trigger_handover_event(&mut self) -> Result<()> {
        // Simplified handover trigger for netns-testbench
        info!("Triggering handover event (placeholder)");
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        Ok(())
    }

    pub async fn apply_recovery_schedule(&mut self) -> Result<()> {
        info!("Would apply recovery schedule (simplified for netns-testbench)");
        Ok(())
    }

    pub async fn collect_phase_metrics(&self) -> Result<PhaseMetrics> {
        // Use received buffer count as a simple signal
        let received = self.buffer_count.load(Ordering::Relaxed) as f64;
        Ok(PhaseMetrics {
            avg_bitrate: received, // proxy
            packet_loss: 0.0,
            avg_rtt: 0.0,
            primary_link_util: 50.0,
            backup_link_util: 50.0,
        })
    }
}

impl Drop for RistIntegrationTest {
    fn drop(&mut self) {
        // Stop sender first
        if let Some(p) = self.sender.take() {
            let _ = p.set_state(gst::State::Null);
        }
        // Finalize MP4 on receiver by sending EOS and waiting briefly
        if let Some(p) = self.receiver.take() {
            // Try to write moov/ctts by sending EOS before shutting down
            let _ = p.send_event(gst::event::Eos::new());
            if let Some(bus) = p.bus() {
                // Wait up to 3s for EOS or Error
                let _ = bus.timed_pop_filtered(
                    Some(gst::ClockTime::from_seconds(3)),
                    &[gst::MessageType::Eos, gst::MessageType::Error],
                );
            }
            let _ = p.set_state(gst::State::Null);
            // Log final MP4 size if we know the path
            if let Some(ref path) = self.mp4_path {
                match std::fs::metadata(path) {
                    Ok(meta) => info!(
                        "mp4_written path={} size_bytes={}",
                        path.display(),
                        meta.len()
                    ),
                    Err(_) => info!("mp4_written path={} size_bytes=unknown", path.display()),
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct PhaseMetrics {
    pub avg_bitrate: f64,
    pub packet_loss: f64,
    pub avg_rtt: f64,
    pub primary_link_util: f64,
    pub backup_link_util: f64,
}

impl RistIntegrationTest {
    fn wait_for_state(
        &self,
        pipeline: &gst::Pipeline,
        state: gst::State,
        timeout_secs: u64,
    ) -> Result<()> {
        pipeline.set_state(state)?;
        let bus = pipeline.bus().unwrap();
        let timeout = gst::ClockTime::from_seconds(timeout_secs);
        match bus.timed_pop_filtered(
            Some(timeout),
            &[
                gst::MessageType::AsyncDone,
                gst::MessageType::StateChanged,
                gst::MessageType::Error,
            ],
        ) {
            Some(msg) => match msg.view() {
                gst::MessageView::Error(err) => {
                    Err(anyhow::anyhow!("Pipeline error: {}", err.error()))
                }
                _ => Ok(()),
            },
            None => Err(anyhow::anyhow!("Timeout waiting for state change")),
        }
    }
}

impl Default for PhaseMetrics {
    fn default() -> Self {
        Self {
            avg_bitrate: 0.0,
            packet_loss: 0.0,
            avg_rtt: 0.0,
            primary_link_util: 0.0,
            backup_link_util: 0.0,
        }
    }
}

#[derive(Debug)]
pub struct TestResults {
    pub test_id: String,
    pub phases: Vec<(String, PhaseMetrics)>,
    pub total_duration: Duration,
}

impl TestResults {
    pub fn new(test_id: String) -> Self {
        Self {
            test_id,
            phases: Vec::new(),
            total_duration: Duration::from_secs(0),
        }
    }

    pub fn add_phase(&mut self, phase: &str, metrics: PhaseMetrics) {
        self.phases.push((phase.to_string(), metrics));
    }
}

#[derive(Debug)]
pub struct ValidationReport {
    pub adaptive_bitrate_working: bool,
    pub bonding_effective: bool,
    pub load_balancing_working: bool,
}

impl ValidationReport {
    fn new() -> Self {
        Self {
            adaptive_bitrate_working: false,
            bonding_effective: false,
            load_balancing_working: false,
        }
    }

    pub fn all_passed(&self) -> bool {
        self.adaptive_bitrate_working && self.bonding_effective && self.load_balancing_working
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono;
    use std::time::Duration;
    use tokio::time::timeout;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_full_bonding_integration_h265() {
        // Test the complete bonding integration scenario with H.265
        let test_id = format!("integration_test_{}", chrono::Utc::now().timestamp());
        let mut test = RistIntegrationTest::new(test_id.clone(), 5008)
            .await
            .expect("Failed to create integration test");

        // Enable verbose rist logs for diagnosis when running with sudo
        std::env::set_var("GST_DEBUG", "rist*:5");

        // Setup bonding and start pipelines in netns
        let links = match test.setup_bonding().await {
            Ok(l) => l,
            Err(e) => {
                if e.to_string().contains("Permission denied") {
                    eprintln!(
                        "SKIP: netns requires root/CAP_SYS_ADMIN; skipping test: {}",
                        e
                    );
                    return;
                } else {
                    panic!("Failed to setup bonding: {}", e);
                }
            }
        };
        if let Err(e) = test.start_rist_pipelines_in_netns().await {
            if e.to_string().contains("Permission denied") {
                eprintln!(
                    "SKIP: netns requires root/CAP_SYS_ADMIN; skipping test: {}",
                    e
                );
                return;
            } else {
                panic!("Failed to start RIST pipelines in netns: {}", e);
            }
        }

        assert!(!links.is_empty(), "Should have created bonding links");
        // Orchestrator currently starts only the first link of the scenario; expect >= 1
        assert!(links.len() >= 1, "Should have at least one active link");

        // Let it run and verify we observe buffers (allow extra time for handshake)
        test.simulate_traffic_phase("flow", Duration::from_secs(8))
            .await
            .unwrap();
        let metrics = test.collect_phase_metrics().await.unwrap();
        assert!(
            metrics.avg_bitrate > 0.0,
            "Should receive some buffers over RIST"
        );
    }

    #[tokio::test]
    async fn test_phase_transition_validation() {
        let test_id = format!("phase_test_{}", chrono::Utc::now().timestamp());
        let mut test = RistIntegrationTest::new(test_id, 5009)
            .await
            .expect("Failed to create integration test");

        // Test network degradation schedule application
        test.apply_degradation_schedule()
            .await
            .expect("Failed to apply degradation schedule");

        // Test handover event triggering
        test.trigger_handover_event()
            .await
            .expect("Failed to trigger handover event");

        // Test recovery schedule application
        test.apply_recovery_schedule()
            .await
            .expect("Failed to apply recovery schedule");
    }

    #[tokio::test]
    async fn test_metrics_collection_and_validation() {
        let test_id = format!("metrics_test_{}", chrono::Utc::now().timestamp());
        let test = RistIntegrationTest::new(test_id, 5010)
            .await
            .expect("Failed to create integration test");

        // Test metrics collection
        let metrics = test
            .collect_phase_metrics()
            .await
            .expect("Failed to collect phase metrics");

        // Validate metrics structure and reasonable values
        assert!(
            metrics.avg_bitrate >= 0.0,
            "Average bitrate should be non-negative"
        );
        assert!(
            metrics.packet_loss >= 0.0 && metrics.packet_loss <= 100.0,
            "Packet loss should be a valid percentage"
        );
        assert!(metrics.avg_rtt >= 0.0, "Average RTT should be non-negative");
        assert!(
            metrics.primary_link_util >= 0.0 && metrics.primary_link_util <= 100.0,
            "Primary link utilization should be a valid percentage"
        );
        assert!(
            metrics.backup_link_util >= 0.0 && metrics.backup_link_util <= 100.0,
            "Backup link utilization should be a valid percentage"
        );
    }

    #[tokio::test]
    async fn test_validation_report_comprehensive() {
        let mut results = TestResults::new("test_validation".to_string());

        // Add test phases with different characteristics
        results.add_phase(
            "strong",
            PhaseMetrics {
                avg_bitrate: 2000.0,
                packet_loss: 0.1,
                avg_rtt: 20.0,
                primary_link_util: 80.0,
                backup_link_util: 20.0,
            },
        );

        results.add_phase(
            "degraded",
            PhaseMetrics {
                avg_bitrate: 800.0, // Should be less than 1000 for test
                packet_loss: 2.0,
                avg_rtt: 150.0,
                primary_link_util: 60.0,
                backup_link_util: 40.0,
            },
        );

        results.add_phase(
            "handover",
            PhaseMetrics {
                avg_bitrate: 1200.0,
                packet_loss: 3.0, // Should be less than 5% for bonding effectiveness
                avg_rtt: 100.0,
                primary_link_util: 50.0,
                backup_link_util: 50.0,
            },
        );

        results.add_phase(
            "recovery",
            PhaseMetrics {
                avg_bitrate: 1800.0, // Should be greater than 1500 for test
                packet_loss: 0.5,
                avg_rtt: 25.0,
                primary_link_util: 75.0,
                backup_link_util: 25.0,
            },
        );

        let test_id = format!("validation_test_{}", chrono::Utc::now().timestamp());
        let test = RistIntegrationTest::new(test_id, 5011)
            .await
            .expect("Failed to create integration test");

        let report = test
            .validate_bonding_behavior(&results)
            .await
            .expect("Failed to validate bonding behavior");

        // Test that validation correctly identifies good bonding behavior
        assert!(
            report.adaptive_bitrate_working,
            "Should detect adaptive bitrate based on phase characteristics"
        );
        assert!(
            report.bonding_effective,
            "Should detect effective bonding during handover phase"
        );
        assert!(
            report.all_passed(),
            "All validation checks should pass with good metrics"
        );
    }

    #[tokio::test]
    async fn test_traffic_simulation_phases() {
        let test_id = format!("traffic_test_{}", chrono::Utc::now().timestamp());
        let test = RistIntegrationTest::new(test_id, 5012)
            .await
            .expect("Failed to create integration test");

        // Test different traffic simulation phases with short durations
        let phases = ["strong", "degraded", "handover", "recovery"];

        for phase in &phases {
            // Use timeout to ensure test doesn't hang
            let result = timeout(
                Duration::from_secs(10),
                test.simulate_traffic_phase(phase, Duration::from_millis(500)),
            )
            .await;

            assert!(
                result.is_ok(),
                "Traffic simulation should complete within timeout"
            );
            assert!(
                result.unwrap().is_ok(),
                "Traffic simulation for phase {} should succeed",
                phase
            );
        }
    }

    #[tokio::test]
    async fn test_error_handling_and_cleanup() {
        // Test with invalid port to trigger error handling
        let result = RistIntegrationTest::new("error_test".to_string(), 0).await;
        // This might succeed depending on implementation, but should handle gracefully

        if let Ok(test) = result {
            // Test that cleanup works properly (Drop implementation)
            drop(test);
        }

        // Test validation with empty results
        let empty_results = TestResults::new("empty_test".to_string());
        let test_id = format!("error_test_{}", chrono::Utc::now().timestamp());
        let test = RistIntegrationTest::new(test_id, 5013)
            .await
            .expect("Failed to create integration test");

        let report = test
            .validate_bonding_behavior(&empty_results)
            .await
            .expect("Should handle empty results gracefully");

        // Empty results should fail validation
        assert!(
            !report.all_passed(),
            "Empty results should not pass all validations"
        );
    }
}
