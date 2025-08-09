#!/usr/bin/env python3
"""
Final report generator - combines results from multiple scenarios
Creates an overall summary for CI job output
"""

import json
import sys
from pathlib import Path
from datetime import datetime


def main():
    if len(sys.argv) != 2:
        print("Usage: generate_final_report.py <results_dir>")
        sys.exit(1)

    results_dir = Path(sys.argv[1])

    if not results_dir.exists():
        print(f"❌ Results directory not found: {results_dir}")
        sys.exit(1)

    # Find all scenario result directories
    scenario_dirs = [d for d in results_dir.iterdir() if d.is_dir()]

    if not scenario_dirs:
        print("❌ No scenario results found")
        sys.exit(1)

    # Process each scenario
    scenarios = []
    overall_pass = True

    for scenario_dir in sorted(scenario_dirs):
        metrics_file = scenario_dir / "metrics.json"

        if not metrics_file.exists():
            print(f"⚠️  Missing metrics.json in {scenario_dir.name}")
            overall_pass = False
            continue

        try:
            with open(metrics_file, "r") as f:
                metrics = json.load(f)

            scenario_pass = (
                metrics.get("verdicts", {}).get("overall", {}).get("pass", False)
            )
            if not scenario_pass:
                overall_pass = False

            scenarios.append(
                {"name": scenario_dir.name, "pass": scenario_pass, "metrics": metrics}
            )

        except Exception as e:
            print(f"❌ Error reading {metrics_file}: {e}")
            overall_pass = False

    # Generate final summary
    summary = generate_summary(scenarios, overall_pass)

    # Write final summary
    final_summary_file = results_dir / "final_summary.md"
    with open(final_summary_file, "w") as f:
        f.write(summary)

    print(f"📋 Final report written to: {final_summary_file}")

    # Exit with appropriate code
    sys.exit(0 if overall_pass else 1)


def generate_summary(scenarios: list, overall_pass: bool) -> str:
    """Generate the final summary markdown"""

    summary = "# RIST Bonding Test Results\n\n"
    summary += f"**Timestamp:** {datetime.now().isoformat()}\n"
    summary += f"**Scenarios Run:** {len(scenarios)}\n\n"

    # Overall status
    overall_emoji = "✅" if overall_pass else "❌"
    overall_status = "PASS" if overall_pass else "FAIL"
    summary += f"## Overall Result: {overall_emoji} {overall_status}\n\n"

    # Scenario results table
    summary += "## Scenario Results\n\n"
    summary += "| Scenario | Status | Delivered Bitrate | Loss % | Max Stall | Utilization Balance |\n"
    summary += "|----------|--------|-------------------|--------|-----------|--------------------|\n"

    for scenario in scenarios:
        name = scenario["name"]
        pass_status = scenario["pass"]
        status_emoji = "✅" if pass_status else "❌"

        # Extract key metrics
        kpis = scenario["metrics"].get("measurements", {}).get("kpis", {})
        bitrate = kpis.get("delivered_bitrate_pct", 0)
        loss = kpis.get("loss_after_recovery_pct", 0)
        stall = kpis.get("max_stall_ms", 0)
        balance = kpis.get("top2_utilization_pct", 0)

        summary += f"| {name} | {status_emoji} | {bitrate:.1f}% | {loss:.2f}% | {stall}ms | {balance:.1f}% |\n"

    # Failed scenarios details
    failed_scenarios = [s for s in scenarios if not s["pass"]]
    if failed_scenarios:
        summary += "\n## Failed Scenarios\n\n"
        for scenario in failed_scenarios:
            summary += f"### {scenario['name']}\n\n"

            verdicts = scenario["metrics"].get("verdicts", {})
            for check_name, verdict in verdicts.items():
                if check_name != "overall" and not verdict.get("pass", False):
                    reason = verdict.get("reason", "Unknown failure")
                    summary += (
                        f"- **{check_name.replace('_', ' ').title()}**: {reason}\n"
                    )

            summary += "\n"

    # Summary statistics
    passed_count = sum(1 for s in scenarios if s["pass"])
    summary += "\n## Summary Statistics\n\n"
    summary += f"- **Total Scenarios:** {len(scenarios)}\n"
    summary += f"- **Passed:** {passed_count}\n"
    summary += f"- **Failed:** {len(scenarios) - passed_count}\n"
    summary += f"- **Success Rate:** {(passed_count / len(scenarios)) * 100:.1f}%\n"

    if not overall_pass:
        summary += "\n## Next Steps\n\n"
        summary += "❌ Some tests failed. Please:\n"
        summary += "1. Review the failed scenario details above\n"
        summary += "2. Check the individual scenario logs in the artifacts\n"
        summary += "3. Verify network configuration and plugin functionality\n"
        summary += (
            "4. Consider adjusting acceptance criteria if failures are expected\n"
        )

    return summary


if __name__ == "__main__":
    main()
