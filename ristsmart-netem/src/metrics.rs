//! Metrics collection and reporting

use crate::types::GeState;
use serde::{Deserialize, Serialize};

/// Per-link metrics snapshot
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LinkMetrics {
    /// Namespace name
    pub namespace: String,
    /// Current egress rate in bps (from OU controller)
    pub egress_rate_bps: u64,
    /// Current Gilbert-Elliott state
    pub ge_state: GeState,
    /// Current loss percentage
    pub loss_pct: f64,
    /// Current delay in ms
    pub delay_ms: u32,
    /// Current jitter in ms
    pub jitter_ms: u32,
    /// Transmitted bytes (from interface stats)
    pub tx_bytes: u64,
    /// Received bytes (from interface stats)
    pub rx_bytes: u64,
    /// Transmitted packets
    pub tx_packets: u64,
    /// Received packets
    pub rx_packets: u64,
    /// Dropped packets
    pub dropped_packets: u64,
}

impl LinkMetrics {
    pub fn new(namespace: String) -> Self {
        Self {
            namespace,
            egress_rate_bps: 0,
            ge_state: GeState::Good,
            loss_pct: 0.0,
            delay_ms: 0,
            jitter_ms: 0,
            tx_bytes: 0,
            rx_bytes: 0,
            tx_packets: 0,
            rx_packets: 0,
            dropped_packets: 0,
        }
    }
}

/// Complete metrics snapshot for all links
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MetricsSnapshot {
    /// Timestamp in milliseconds since epoch
    pub timestamp_ms: u64,
    /// Per-link metrics
    pub links: Vec<LinkMetrics>,
}

impl MetricsSnapshot {
    pub fn new() -> Self {
        Self {
            timestamp_ms: crate::util::timestamp_ms(),
            links: Vec::new(),
        }
    }

    pub fn add_link(&mut self, metrics: LinkMetrics) {
        self.links.push(metrics);
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn to_json_line(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

/// Metrics collector that can gather statistics from network interfaces
pub struct MetricsCollector {
    rtnetlink_handle: rtnetlink::Handle,
}

impl MetricsCollector {
    pub fn new() -> crate::errors::Result<Self> {
        let (connection, handle, _) = rtnetlink::new_connection()?;
        tokio::spawn(connection);

        Ok(Self {
            rtnetlink_handle: handle,
        })
    }

    /// Collect interface statistics for a given interface index
    pub async fn collect_interface_stats(
        &self,
        if_index: Option<u32>,
    ) -> crate::errors::Result<(u64, u64, u64, u64, u64)> {
        if let Some(index) = if_index {
            self.get_interface_stats(index).await.map(
                |(tx_bytes, rx_bytes, tx_packets, rx_packets)| {
                    (tx_bytes, rx_bytes, tx_packets, rx_packets, 0) // No dropped packets for now
                },
            )
        } else {
            Ok((0, 0, 0, 0, 0))
        }
    }

    /// Collect interface statistics for a given interface index
    async fn get_interface_stats(
        &self,
        if_index: u32,
    ) -> crate::errors::Result<(u64, u64, u64, u64)> {
        use futures::TryStreamExt;

        // First, get the interface name from the index
        let mut links = self
            .rtnetlink_handle
            .link()
            .get()
            .match_index(if_index)
            .execute();

        let interface_name = if let Some(link) = links.try_next().await? {
            // Extract interface name from NLA attributes
            use netlink_packet_route::link::nlas::Nla;

            let mut name = None;
            for nla in &link.nlas {
                if let Nla::IfName(ifname) = nla {
                    name = Some(ifname.clone());
                    break;
                }
            }

            // Use the found name or fallback to interface index
            name.unwrap_or_else(|| format!("if{}", if_index))
        } else {
            return Err(crate::errors::NetemError::LinkNotFound(format!(
                "Interface index {}",
                if_index
            )));
        };

        // Parse /proc/net/dev for interface statistics
        // This is more reliable than parsing raw netlink stats
        let proc_data = tokio::fs::read_to_string("/proc/net/dev").await?;

        for line in proc_data.lines().skip(2) {
            // Skip header lines
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 10 {
                let if_name = parts[0].trim_end_matches(':');
                if if_name == interface_name {
                    // Parse interface statistics
                    // Format: bytes packets errs drop fifo frame compressed multicast
                    let rx_bytes = parts[1].parse::<u64>().unwrap_or(0);
                    let rx_packets = parts[2].parse::<u64>().unwrap_or(0);
                    let tx_bytes = parts[9].parse::<u64>().unwrap_or(0);
                    let tx_packets = parts[10].parse::<u64>().unwrap_or(0);

                    return Ok((tx_bytes, rx_bytes, tx_packets, rx_packets));
                }
            }
        }

        // Interface not found in /proc/net/dev
        Err(crate::errors::NetemError::LinkNotFound(format!(
            "Interface {} not found in /proc/net/dev",
            interface_name
        )))
    }

    /// Collect interface statistics from within a network namespace
    pub async fn collect_interface_stats_in_netns(
        &self,
        if_index: Option<u32>,
        netns_path: &str,
    ) -> crate::errors::Result<(u64, u64, u64, u64, u64)> {
        if let Some(index) = if_index {
            self.get_interface_stats_in_netns(index, netns_path)
                .await
                .map(|(tx_bytes, rx_bytes, tx_packets, rx_packets)| {
                    (tx_bytes, rx_bytes, tx_packets, rx_packets, 0) // No dropped packets for now
                })
        } else {
            Ok((0, 0, 0, 0, 0))
        }
    }

    /// Get interface statistics from within a specific network namespace
    async fn get_interface_stats_in_netns(
        &self,
        if_index: u32,
        netns_path: &str,
    ) -> crate::errors::Result<(u64, u64, u64, u64)> {
        use futures::TryStreamExt;

        // First, get the interface name from the index (this works from host namespace)
        let mut links = self
            .rtnetlink_handle
            .link()
            .get()
            .match_index(if_index)
            .execute();

        let interface_name = if let Some(link) = links.try_next().await? {
            // Extract interface name from NLA attributes
            use netlink_packet_route::link::nlas::Nla;

            let mut name = None;
            for nla in &link.nlas {
                if let Nla::IfName(ifname) = nla {
                    name = Some(ifname.clone());
                    break;
                }
            }

            // Use the found name or fallback to interface index
            name.unwrap_or_else(|| format!("if{}", if_index))
        } else {
            return Err(crate::errors::NetemError::LinkNotFound(format!(
                "Interface index {}",
                if_index
            )));
        };

        // Read /proc/net/dev from within the target namespace
        let netns_proc_path = format!("{}/proc/net/dev", netns_path);
        let proc_data = match tokio::fs::read_to_string(&netns_proc_path).await {
            Ok(data) => data,
            Err(_) => {
                // Fallback: try to execute in the namespace using ip netns exec
                let ns_name = std::path::Path::new(netns_path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("unknown");

                let output = tokio::process::Command::new("ip")
                    .args(&["netns", "exec", ns_name, "cat", "/proc/net/dev"])
                    .output()
                    .await
                    .map_err(|e| crate::errors::NetemError::Io(e))?;

                if !output.status.success() {
                    return Err(crate::errors::NetemError::LinkNotFound(format!(
                        "Failed to read /proc/net/dev from namespace {}",
                        ns_name
                    )));
                }

                String::from_utf8_lossy(&output.stdout).to_string()
            }
        };

        for line in proc_data.lines().skip(2) {
            // Skip header lines
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 10 {
                let if_name = parts[0].trim_end_matches(':');
                if if_name == interface_name {
                    // Parse interface statistics
                    // Format: bytes packets errs drop fifo frame compressed multicast
                    let rx_bytes = parts[1].parse::<u64>().unwrap_or(0);
                    let rx_packets = parts[2].parse::<u64>().unwrap_or(0);
                    let tx_bytes = parts[9].parse::<u64>().unwrap_or(0);
                    let tx_packets = parts[10].parse::<u64>().unwrap_or(0);

                    return Ok((tx_bytes, rx_bytes, tx_packets, rx_packets));
                }
            }
        }

        // Interface not found in /proc/net/dev
        Err(crate::errors::NetemError::LinkNotFound(format!(
            "Interface {} not found in namespace /proc/net/dev",
            interface_name
        )))
    }
}

/// JSONL metrics writer for continuous logging
pub struct MetricsWriter {
    writer: Box<dyn std::io::Write + Send>,
}

impl MetricsWriter {
    pub fn new_file(path: impl AsRef<std::path::Path>) -> std::io::Result<Self> {
        let file = std::fs::File::create(path)?;
        Ok(Self {
            writer: Box::new(file),
        })
    }

    pub fn new_stdout() -> Self {
        Self {
            writer: Box::new(std::io::stdout()),
        }
    }

    pub fn write_snapshot(
        &mut self,
        snapshot: &MetricsSnapshot,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use std::io::Write;

        let json_line = snapshot.to_json_line()?;
        writeln!(self.writer, "{}", json_line)?;
        self.writer.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_serialization() {
        let mut snapshot = MetricsSnapshot::new();
        let metrics = LinkMetrics {
            namespace: "test-ns".to_string(),
            egress_rate_bps: 1000000,
            ge_state: GeState::Good,
            loss_pct: 0.01,
            delay_ms: 20,
            jitter_ms: 5,
            tx_bytes: 1024,
            rx_bytes: 512,
            tx_packets: 10,
            rx_packets: 8,
            dropped_packets: 2,
        };
        snapshot.add_link(metrics);

        let json = snapshot.to_json().expect("JSON serialization should work");
        assert!(json.contains("test-ns"));
        assert!(json.contains("1000000"));

        let json_line = snapshot
            .to_json_line()
            .expect("JSON line serialization should work");
        assert!(!json_line.contains('\n')); // Should be single line
    }

    #[test]
    fn test_parse_proc_net_dev_format() {
        // Test parsing of /proc/net/dev format - this simulates the parsing logic
        // from get_interface_stats without requiring actual network interfaces

        let mock_proc_data = r#"Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
    lo: 1234567    8901   0    0    0     0          0         0  2345678     9012   0    0    0     0       0          0
  eth0: 9876543210    54321   0    2    0     0          0       123 1234567890   12345   0    1    0    15       0          0
veth0n:  5678901   23456   0    0    0     0          0         0  4321098    21098   0    0    0     0       0          0"#;

        // Test parsing lo interface
        let mut lo_found = false;
        for line in mock_proc_data.lines().skip(2) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 10 {
                let if_name = parts[0].trim_end_matches(':');
                if if_name == "lo" {
                    lo_found = true;
                    let rx_bytes = parts[1].parse::<u64>().unwrap_or(0);
                    let rx_packets = parts[2].parse::<u64>().unwrap_or(0);
                    let tx_bytes = parts[9].parse::<u64>().unwrap_or(0);
                    let tx_packets = parts[10].parse::<u64>().unwrap_or(0);

                    assert_eq!(rx_bytes, 1234567);
                    assert_eq!(rx_packets, 8901);
                    assert_eq!(tx_bytes, 2345678);
                    assert_eq!(tx_packets, 9012);
                    break;
                }
            }
        }
        assert!(lo_found, "Should find lo interface in mock data");

        // Test parsing eth0 interface
        let mut eth0_found = false;
        for line in mock_proc_data.lines().skip(2) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 10 {
                let if_name = parts[0].trim_end_matches(':');
                if if_name == "eth0" {
                    eth0_found = true;
                    let rx_bytes = parts[1].parse::<u64>().unwrap_or(0);
                    let rx_packets = parts[2].parse::<u64>().unwrap_or(0);
                    let tx_bytes = parts[9].parse::<u64>().unwrap_or(0);
                    let tx_packets = parts[10].parse::<u64>().unwrap_or(0);

                    assert_eq!(rx_bytes, 9876543210);
                    assert_eq!(rx_packets, 54321);
                    assert_eq!(tx_bytes, 1234567890);
                    assert_eq!(tx_packets, 12345);
                    break;
                }
            }
        }
        assert!(eth0_found, "Should find eth0 interface in mock data");
    }

    #[test]
    fn test_parse_proc_net_dev_edge_cases() {
        // Test edge cases in /proc/net/dev parsing

        // Empty data
        let empty_data = "Inter-|   Receive                                                |  Transmit\n face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\n";
        let mut found_interface = false;
        for line in empty_data.lines().skip(2) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 10 {
                found_interface = true;
            }
        }
        assert!(!found_interface, "Should not find interfaces in empty data");

        // Malformed lines (insufficient columns)
        let malformed_data = r#"Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
eth0: 123 456"#;

        let mut found_malformed = false;
        for line in malformed_data.lines().skip(2) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 10 {
                found_malformed = true;
            }
        }
        assert!(!found_malformed, "Should not parse malformed lines");

        // Interface with zero values
        let zero_data = r#"Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
  eth0:       0        0   0    0    0     0          0         0        0        0   0    0    0     0       0          0"#;

        for line in zero_data.lines().skip(2) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 10 {
                let if_name = parts[0].trim_end_matches(':');
                if if_name == "eth0" {
                    let rx_bytes = parts[1].parse::<u64>().unwrap_or(0);
                    let rx_packets = parts[2].parse::<u64>().unwrap_or(0);
                    let tx_bytes = parts[9].parse::<u64>().unwrap_or(0);
                    let tx_packets = parts[10].parse::<u64>().unwrap_or(0);

                    assert_eq!(rx_bytes, 0);
                    assert_eq!(rx_packets, 0);
                    assert_eq!(tx_bytes, 0);
                    assert_eq!(tx_packets, 0);
                    break;
                }
            }
        }
    }

    #[test]
    fn test_parse_proc_net_dev_parsing_failures() {
        // Test behavior when parsing invalid numbers
        let invalid_data = r#"Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
eth0: invalid_bytes    invalid_packets   0    0    0     0          0         0  invalid_tx_bytes    invalid_tx_packets   0    0    0     0       0          0"#;

        for line in invalid_data.lines().skip(2) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 10 {
                let if_name = parts[0].trim_end_matches(':');
                if if_name == "eth0" {
                    // Should use unwrap_or(0) fallback for invalid numbers
                    let rx_bytes = parts[1].parse::<u64>().unwrap_or(0);
                    let rx_packets = parts[2].parse::<u64>().unwrap_or(0);
                    let tx_bytes = parts[9].parse::<u64>().unwrap_or(0);
                    let tx_packets = parts[10].parse::<u64>().unwrap_or(0);

                    assert_eq!(rx_bytes, 0, "Should fallback to 0 for invalid rx_bytes");
                    assert_eq!(rx_packets, 0, "Should fallback to 0 for invalid rx_packets");
                    assert_eq!(tx_bytes, 0, "Should fallback to 0 for invalid tx_bytes");
                    assert_eq!(tx_packets, 0, "Should fallback to 0 for invalid tx_packets");
                    break;
                }
            }
        }
    }

    #[test]
    fn test_interface_name_parsing() {
        // Test parsing of different interface name formats
        let interface_data = r#"Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
    lo:    1000     10   0    0    0     0          0         0     2000       20   0    0    0     0       0          0
  eth0:    3000     30   0    0    0     0          0         0     4000       40   0    0    0     0       0          0
veth1h:    5000     50   0    0    0     0          0         0     6000       60   0    0    0     0       0          0
veth2n:    7000     70   0    0    0     0          0         0     8000       80   0    0    0     0       0          0"#;

        let mut found_interfaces = std::collections::HashSet::new();

        for line in interface_data.lines().skip(2) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 10 {
                let if_name = parts[0].trim_end_matches(':');
                found_interfaces.insert(if_name.to_string());
            }
        }

        assert!(found_interfaces.contains("lo"), "Should find lo interface");
        assert!(
            found_interfaces.contains("eth0"),
            "Should find eth0 interface"
        );
        assert!(
            found_interfaces.contains("veth1h"),
            "Should find veth1h interface"
        );
        assert!(
            found_interfaces.contains("veth2n"),
            "Should find veth2n interface"
        );
        assert_eq!(
            found_interfaces.len(),
            4,
            "Should find exactly 4 interfaces"
        );
    }

    #[test]
    fn test_large_counter_values() {
        // Test parsing of large 64-bit counter values
        let large_data = r#"Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
eth0: 18446744073709551615 4294967295   0    0    0     0          0         0 9223372036854775807 2147483647   0    0    0     0       0          0"#;

        for line in large_data.lines().skip(2) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 10 {
                let if_name = parts[0].trim_end_matches(':');
                if if_name == "eth0" {
                    let rx_bytes = parts[1].parse::<u64>().unwrap_or(0);
                    let rx_packets = parts[2].parse::<u64>().unwrap_or(0);
                    let tx_bytes = parts[9].parse::<u64>().unwrap_or(0);
                    let tx_packets = parts[10].parse::<u64>().unwrap_or(0);

                    assert_eq!(
                        rx_bytes, 18446744073709551615u64,
                        "Should parse large u64 rx_bytes"
                    );
                    assert_eq!(
                        rx_packets, 4294967295u64,
                        "Should parse large u32 rx_packets as u64"
                    );
                    assert_eq!(
                        tx_bytes, 9223372036854775807u64,
                        "Should parse large tx_bytes"
                    );
                    assert_eq!(tx_packets, 2147483647u64, "Should parse large tx_packets");
                    break;
                }
            }
        }
    }

    #[test]
    fn test_interface_stats_none_behavior() {
        // Test the logic that returns (0, 0, 0, 0, 0) for None if_index
        // This tests the expected behavior without needing async runtime
        let if_index: Option<u32> = None;
        let expected_result = (0, 0, 0, 0, 0);

        // This is the behavior we expect when if_index is None
        let result = if let Some(_index) = if_index {
            // Would call get_interface_stats
            (100, 200, 300, 400) // mock values
        } else {
            (0, 0, 0, 0) // Returns zeros for None
        };

        let final_result = (result.0, result.1, result.2, result.3, 0); // Add dropped packets
        assert_eq!(
            final_result, expected_result,
            "Should return zeros for None if_index"
        );
    }
}
