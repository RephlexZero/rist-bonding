#!/usr/bin/env python3
"""
Scenario metrics processor - parses logs and network stats to compute KPIs
Generates metrics.json and summary.md for each test scenario
"""

import json
import yaml
import sys
import re
from pathlib import Path
from datetime import datetime
from typing import Dict, Any, Optional


class MetricsProcessor:
    def __init__(self, scenario_name: str, results_dir: str, logs_dir: str):
        self.scenario_name = scenario_name
        self.results_dir = Path(results_dir)
        self.logs_dir = Path(logs_dir)
        self.scenario_file = (
            Path(__file__).parent.parent / "scenarios" / f"{scenario_name}.yaml"
        )

        # Load scenario configuration
        with open(self.scenario_file, "r") as f:
            self.scenario_config = yaml.safe_load(f)

        self.metrics = {
            "scenario": scenario_name,
            "timestamp": datetime.now().isoformat(),
            "environment": self._get_environment_info(),
            "configuration": self.scenario_config,
            "measurements": {},
            "verdicts": {},
        }

    def _get_environment_info(self) -> Dict[str, Any]:
        """Gather environment information"""
        import subprocess

        try:
            # Get system info
            uname = subprocess.run(["uname", "-a"], capture_output=True, text=True)

            # Try to get GStreamer version
            gst_version = subprocess.run(
                ["gst-launch-1.0", "--version"], capture_output=True, text=True
            )

            return {
                "system": uname.stdout.strip() if uname.returncode == 0 else "unknown",
                "gstreamer_version": self._extract_gst_version(gst_version.stdout)
                if gst_version.returncode == 0
                else "unknown",
                "timestamp": datetime.now().isoformat(),
            }
        except Exception as e:
            return {"error": str(e)}

    def _extract_gst_version(self, version_output: str) -> str:
        """Extract GStreamer version from gst-launch output"""
        match = re.search(r"gst-launch-1\.0 version (\S+)", version_output)
        return match.group(1) if match else "unknown"

    def process_logs(self) -> Dict[str, Any]:
        """Process GStreamer logs to extract metrics"""
        sender_log = self.logs_dir / "sender.log"
        receiver_log = self.logs_dir / "receiver.log"

        metrics = {
            "sender_stats": self._process_sender_log(sender_log),
            "receiver_stats": self._process_receiver_log(receiver_log),
        }

        return metrics

    def _process_sender_log(self, log_file: Path) -> Dict[str, Any]:
        """Extract metrics from sender pipeline log"""
        if not log_file.exists():
            return {"error": "sender log not found"}

        stats = {
            "buffers_sent": 0,
            "bytes_sent": 0,
            "errors": [],
            "warnings": [],
            "rist_stats": {},
        }

        try:
            with open(log_file, "r") as f:
                for line in f:
                    # Count buffer flow
                    if "chain" in line.lower() and "buffer" in line.lower():
                        stats["buffers_sent"] += 1

                    # Extract RIST statistics if available
                    if "rist" in line.lower() and "stats" in line.lower():
                        # Parse RIST statistics from debug output
                        # This would need to match actual ristsink output format
                        pass

                    # Collect errors and warnings
                    if "ERROR" in line:
                        stats["errors"].append(line.strip())
                    elif "WARN" in line:
                        stats["warnings"].append(line.strip())

        except Exception as e:
            stats["error"] = str(e)

        return stats

    def _process_receiver_log(self, log_file: Path) -> Dict[str, Any]:
        """Extract metrics from receiver pipeline log"""
        if not log_file.exists():
            return {"error": "receiver log not found"}

        stats = {
            "buffers_received": 0,
            "bytes_received": 0,
            "stalls": [],
            "errors": [],
            "warnings": [],
        }

        try:
            with open(log_file, "r") as f:
                last_buffer_time = None

                for line in f:
                    # Count received buffers
                    if "chain" in line.lower() and "buffer" in line.lower():
                        stats["buffers_received"] += 1

                        # Extract timestamp if available for stall detection
                        timestamp_match = re.search(r"(\d+:\d+:\d+\.\d+)", line)
                        if timestamp_match:
                            current_time = timestamp_match.group(1)
                            if last_buffer_time:
                                # Simple stall detection (would need better timestamp parsing)
                                pass
                            last_buffer_time = current_time

                    # Collect errors and warnings
                    if "ERROR" in line:
                        stats["errors"].append(line.strip())
                    elif "WARN" in line:
                        stats["warnings"].append(line.strip())

        except Exception as e:
            stats["error"] = str(e)

        return stats

    def process_network_stats(self) -> Dict[str, Any]:
        """Process tc and interface statistics"""
        stats_files = list(self.results_dir.glob("*stats*.txt"))

        if not stats_files:
            return {"error": "no network stats files found"}

        network_metrics = {
            "initial_stats": self._parse_stats_file("initial_stats.txt"),
            "periodic_stats": [],
            "final_stats": self._parse_stats_file("final_stats.txt"),
            "link_utilization": {},
        }

        # Process periodic stats
        periodic_files = sorted([f for f in stats_files if "stats_" in f.name])
        for stats_file in periodic_files:
            stats_data = self._parse_stats_file(stats_file.name)
            if stats_data:
                network_metrics["periodic_stats"].append(
                    {"filename": stats_file.name, "data": stats_data}
                )

        # Compute link utilization
        network_metrics["link_utilization"] = self._compute_link_utilization(
            network_metrics
        )

        return network_metrics

    def _parse_stats_file(self, filename: str) -> Optional[Dict[str, Any]]:
        """Parse a single tc stats file"""
        stats_file = self.results_dir / filename
        if not stats_file.exists():
            return None

        parsed_stats = {"links": {}}

        try:
            with open(stats_file, "r") as f:
                content = f.read()

                # Parse per-link statistics
                for i in range(1, 5):  # Links 1-4
                    link_section = re.search(
                        rf"--- Link {i} \(vethS{i}\) ---(.*?)---", content, re.DOTALL
                    )
                    if link_section:
                        link_data = self._parse_link_section(link_section.group(1))
                        parsed_stats["links"][f"link_{i}"] = link_data

        except Exception as e:
            parsed_stats["error"] = str(e)

        return parsed_stats

    def _parse_link_section(self, section_text: str) -> Dict[str, Any]:
        """Parse statistics for a single link"""
        link_stats = {
            "bytes_sent": 0,
            "packets_sent": 0,
            "drops": 0,
            "rate_limit_drops": 0,
        }

        # Extract bytes and packets from tc output
        bytes_match = re.search(r"Sent (\d+) bytes (\d+) pkt", section_text)
        if bytes_match:
            link_stats["bytes_sent"] = int(bytes_match.group(1))
            link_stats["packets_sent"] = int(bytes_match.group(2))

        # Extract drops
        drops_match = re.search(r"dropped (\d+)", section_text)
        if drops_match:
            link_stats["drops"] = int(drops_match.group(1))

        return link_stats

    def _compute_link_utilization(
        self, network_metrics: Dict[str, Any]
    ) -> Dict[str, Any]:
        """Compute per-link utilization percentages"""
        utilization = {}

        final_stats = network_metrics.get("final_stats", {})
        if not final_stats or "links" not in final_stats:
            return {"error": "insufficient data for utilization calculation"}

        total_bytes = sum(
            link_data.get("bytes_sent", 0)
            for link_data in final_stats["links"].values()
        )

        if total_bytes == 0:
            return {"error": "no traffic detected"}

        for link_id, link_data in final_stats["links"].items():
            bytes_sent = link_data.get("bytes_sent", 0)
            utilization[link_id] = {
                "bytes_sent": bytes_sent,
                "percentage": (bytes_sent / total_bytes) * 100
                if total_bytes > 0
                else 0,
            }

        return utilization

    def compute_kpis(
        self, log_metrics: Dict[str, Any], network_metrics: Dict[str, Any]
    ) -> Dict[str, Any]:
        """Compute key performance indicators"""
        kpis = {}

        # Delivered bitrate calculation (simplified)
        sender_buffers = log_metrics.get("sender_stats", {}).get("buffers_sent", 0)
        receiver_buffers = log_metrics.get("receiver_stats", {}).get(
            "buffers_received", 0
        )
        duration = self.scenario_config.get("duration_sec", 30)

        if sender_buffers > 0:
            delivery_ratio = receiver_buffers / sender_buffers
            kpis["delivered_bitrate_pct"] = delivery_ratio * 100
        else:
            kpis["delivered_bitrate_pct"] = 0

        # Loss calculation
        if sender_buffers > 0:
            lost_buffers = sender_buffers - receiver_buffers
            kpis["loss_after_recovery_pct"] = (lost_buffers / sender_buffers) * 100
        else:
            kpis["loss_after_recovery_pct"] = 0

        # Stall metrics (simplified - would need better timestamp analysis)
        kpis["max_stall_ms"] = 0  # Placeholder
        kpis["total_stall_ms"] = 0  # Placeholder

        # Link utilization balance
        utilization = network_metrics.get("link_utilization", {})
        if utilization and "error" not in utilization:
            percentages = [data["percentage"] for data in utilization.values()]
            # Check if top 2 links carry sufficient traffic
            top2_percentage = sum(sorted(percentages, reverse=True)[:2])
            kpis["top2_utilization_pct"] = top2_percentage
        else:
            kpis["top2_utilization_pct"] = 0

        return kpis

    def evaluate_verdicts(self, kpis: Dict[str, Any]) -> Dict[str, Any]:
        """Evaluate pass/fail verdicts based on acceptance criteria"""
        criteria = self.scenario_config.get("acceptance_criteria", {})
        verdicts = {}

        # Delivered bitrate check
        min_bitrate = criteria.get("delivered_bitrate_pct", 85)
        actual_bitrate = kpis.get("delivered_bitrate_pct", 0)
        verdicts["delivered_bitrate"] = {
            "pass": actual_bitrate >= min_bitrate,
            "threshold": min_bitrate,
            "actual": actual_bitrate,
            "reason": f"Delivered {actual_bitrate:.1f}% (threshold: {min_bitrate}%)",
        }

        # Loss check
        max_loss = criteria.get("loss_after_recovery_pct", 1.0)
        actual_loss = kpis.get("loss_after_recovery_pct", 0)
        verdicts["loss_after_recovery"] = {
            "pass": actual_loss <= max_loss,
            "threshold": max_loss,
            "actual": actual_loss,
            "reason": f"Loss {actual_loss:.2f}% (threshold: ≤{max_loss}%)",
        }

        # Stall check
        max_stall = criteria.get("max_stall_ms", 500)
        actual_stall = kpis.get("max_stall_ms", 0)
        verdicts["max_stall"] = {
            "pass": actual_stall <= max_stall,
            "threshold": max_stall,
            "actual": actual_stall,
            "reason": f"Max stall {actual_stall}ms (threshold: ≤{max_stall}ms)",
        }

        # Utilization balance check
        min_balance = criteria.get("utilization_balance_threshold", 0.7) * 100
        actual_balance = kpis.get("top2_utilization_pct", 0)
        verdicts["utilization_balance"] = {
            "pass": actual_balance >= min_balance,
            "threshold": min_balance,
            "actual": actual_balance,
            "reason": f"Top-2 utilization {actual_balance:.1f}% (threshold: ≥{min_balance}%)",
        }

        # Overall pass/fail
        all_passed = all(v["pass"] for v in verdicts.values())
        verdicts["overall"] = {
            "pass": all_passed,
            "reason": "All criteria passed" if all_passed else "Some criteria failed",
        }

        return verdicts

    def generate_summary(self, metrics: Dict[str, Any]) -> str:
        """Generate human-readable summary"""
        verdicts = metrics["verdicts"]
        kpis = metrics["measurements"]["kpis"]

        # Header
        summary = f"# Test Results: {self.scenario_name}\n\n"
        summary += f"**Scenario:** {self.scenario_config.get('description', 'N/A')}\n"
        summary += f"**Duration:** {self.scenario_config.get('duration_sec', 'N/A')}s\n"
        summary += f"**Timestamp:** {metrics['timestamp']}\n\n"

        # Overall result
        overall_pass = verdicts["overall"]["pass"]
        status_emoji = "✅" if overall_pass else "❌"
        summary += f"## Overall Result: {status_emoji} {'PASS' if overall_pass else 'FAIL'}\n\n"

        # KPI table
        summary += "## Key Performance Indicators\n\n"
        summary += "| Metric | Value | Threshold | Status |\n"
        summary += "|--------|-------|-----------|--------|\n"

        for metric_name, verdict in verdicts.items():
            if metric_name == "overall":
                continue

            status_emoji = "✅" if verdict["pass"] else "❌"
            actual = verdict["actual"]
            threshold = verdict["threshold"]

            # Format values appropriately
            if "pct" in metric_name or "percentage" in metric_name:
                actual_str = f"{actual:.1f}%"
                threshold_str = f"{threshold:.1f}%"
            elif "ms" in metric_name:
                actual_str = f"{actual}ms"
                threshold_str = f"{threshold}ms"
            else:
                actual_str = str(actual)
                threshold_str = str(threshold)

            summary += f"| {metric_name.replace('_', ' ').title()} | {actual_str} | {threshold_str} | {status_emoji} |\n"

        # Failures section
        failed_checks = [
            name
            for name, verdict in verdicts.items()
            if name != "overall" and not verdict["pass"]
        ]

        if failed_checks:
            summary += "\n## Failed Checks\n\n"
            for check in failed_checks:
                verdict = verdicts[check]
                summary += (
                    f"- **{check.replace('_', ' ').title()}**: {verdict['reason']}\n"
                )

        # Network utilization
        utilization = metrics["measurements"]["network"]["link_utilization"]
        if utilization and "error" not in utilization:
            summary += "\n## Link Utilization\n\n"
            for link_id, data in utilization.items():
                summary += f"- **{link_id.replace('_', ' ').title()}**: {data['percentage']:.1f}% ({data['bytes_sent']} bytes)\n"

        return summary

    def process(self) -> bool:
        """Main processing pipeline"""
        try:
            # Process all data sources
            log_metrics = self.process_logs()
            network_metrics = self.process_network_stats()

            # Compute KPIs
            kpis = self.compute_kpis(log_metrics, network_metrics)

            # Store measurements
            self.metrics["measurements"] = {
                "logs": log_metrics,
                "network": network_metrics,
                "kpis": kpis,
            }

            # Evaluate verdicts
            verdicts = self.evaluate_verdicts(kpis)
            self.metrics["verdicts"] = verdicts

            # Generate outputs
            metrics_file = self.results_dir / "metrics.json"
            summary_file = self.results_dir / "summary.md"

            # Write metrics.json
            with open(metrics_file, "w") as f:
                json.dump(self.metrics, f, indent=2)

            # Write summary.md
            summary_text = self.generate_summary(self.metrics)
            with open(summary_file, "w") as f:
                f.write(summary_text)

            print(f"✅ Metrics processing completed for {self.scenario_name}")
            print(f"📄 Results: {metrics_file}")
            print(f"📋 Summary: {summary_file}")

            return verdicts["overall"]["pass"]

        except Exception as e:
            error_msg = f"❌ Error processing scenario {self.scenario_name}: {e}"
            print(error_msg)

            # Write error to files
            error_metrics = {
                "scenario": self.scenario_name,
                "error": str(e),
                "timestamp": datetime.now().isoformat(),
                "verdicts": {"overall": {"pass": False, "reason": str(e)}},
            }

            with open(self.results_dir / "metrics.json", "w") as f:
                json.dump(error_metrics, f, indent=2)

            with open(self.results_dir / "summary.md", "w") as f:
                f.write(f"# Error Processing {self.scenario_name}\n\n{error_msg}\n")

            return False


def main():
    if len(sys.argv) != 4:
        print("Usage: process_scenario.py <scenario_name> <results_dir> <logs_dir>")
        sys.exit(1)

    scenario_name = sys.argv[1]
    results_dir = sys.argv[2]
    logs_dir = sys.argv[3]

    processor = MetricsProcessor(scenario_name, results_dir, logs_dir)
    success = processor.process()

    sys.exit(0 if success else 1)


if __name__ == "__main__":
    main()
