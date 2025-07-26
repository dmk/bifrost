#!/usr/bin/env python3

import os
import re
from pathlib import Path

def parse_memtier_results(file_path):
    """Parse memtier_benchmark text output from the BEST RUN RESULTS section."""
    with open(file_path, 'r') as f:
        content = f.read()

    # Find the BEST RUN RESULTS section and extract the Totals line
    # Format: Totals    Ops/sec    Hits/sec   Misses/sec   Avg.Latency   p50   p90   p95   p99   p99.9   KB/sec
    totals_pattern = r'Totals\s+(\d+\.\d+)\s+\S+\s+\S+\s+(\d+\.\d+)\s+(\d+\.\d+)\s+(\d+\.\d+)\s+(\d+\.\d+)\s+(\d+\.\d+)\s+(\d+\.\d+)'
    match = re.search(totals_pattern, content)

    if match:
        return {
            'throughput': float(match.group(1)),
            'avg_latency': float(match.group(2)),
            'p50_latency': float(match.group(3)),
            'p90_latency': float(match.group(4)),
            'p95_latency': float(match.group(5)),
            'p99_latency': float(match.group(6)),
            'p999_latency': float(match.group(7))
        }
    return None

def analyze_benchmarks(results_dir):
    """Analyze all benchmark results and create comparison."""
    results_path = Path(results_dir)

    # Get all unique test scenarios from file names
    scenarios = set()
    for file in results_path.glob("*.txt"):
        # Extract scenario from filename like "bifrost_light_read_heavy.txt"
        if file.name.startswith(('bifrost_', 'mcrouter_')):
            scenario = file.name.split('_', 1)[1].replace('.txt', '')
            scenarios.add(scenario)

    scenarios = sorted(scenarios)

    print("🚀 Bifrost vs MCRouter Benchmark Analysis")
    print("=" * 50)
    print()

    summary_data = []
    bifrost_wins = 0
    total_comparisons = 0

    for scenario in scenarios:
        bifrost_file = results_path / f"bifrost_{scenario}.txt"
        mcrouter_file = results_path / f"mcrouter_{scenario}.txt"

        if bifrost_file.exists() and mcrouter_file.exists():
            bifrost_data = parse_memtier_results(bifrost_file)
            mcrouter_data = parse_memtier_results(mcrouter_file)

            if bifrost_data and mcrouter_data:
                total_comparisons += 1
                scenario_name = scenario.replace('_', ' ').title()

                print(f"📊 {scenario_name}")
                print("-" * 30)

                # Throughput comparison
                bifrost_ops = bifrost_data['throughput']
                mcrouter_ops = mcrouter_data['throughput']
                throughput_diff = ((bifrost_ops - mcrouter_ops) / mcrouter_ops) * 100

                print(f"Throughput (ops/sec):")
                print(f"  Bifrost:  {bifrost_ops:>10,.0f}")
                print(f"  MCRouter: {mcrouter_ops:>10,.0f}")
                print(f"  Difference: {throughput_diff:>+7.1f}%")

                # Latency comparison
                print(f"\nLatency (ms):")
                print(f"  Metric    Bifrost   MCRouter   Difference")
                print(f"  ------    -------   --------   ----------")

                latency_metrics = [
                    ('P50', 'p50_latency'),
                    ('P95', 'p95_latency'),
                    ('P99', 'p99_latency')
                ]

                for name, key in latency_metrics:
                    bifrost_lat = bifrost_data[key]
                    mcrouter_lat = mcrouter_data[key]
                    lat_diff = ((bifrost_lat - mcrouter_lat) / mcrouter_lat) * 100
                    print(f"  {name:<6}    {bifrost_lat:>7.3f}   {mcrouter_lat:>8.3f}   {lat_diff:>+7.1f}%")

                # Determine winner
                if bifrost_ops > mcrouter_ops:
                    winner = "🟢 Bifrost"
                    bifrost_wins += 1
                else:
                    winner = "🔴 MCRouter"

                print(f"\n🏆 Winner: {winner}")
                print()

                # Store for summary
                summary_data.append({
                    'scenario': scenario_name,
                    'bifrost_ops': bifrost_ops,
                    'mcrouter_ops': mcrouter_ops,
                    'throughput_diff': throughput_diff,
                    'bifrost_p95': bifrost_data['p95_latency'],
                    'mcrouter_p95': mcrouter_data['p95_latency'],
                    'winner': 'Bifrost' if bifrost_ops > mcrouter_ops else 'MCRouter'
                })

    # Overall summary
    print("🏁 OVERALL SUMMARY")
    print("=" * 50)
    print(f"Total scenarios tested: {total_comparisons}")
    print(f"Bifrost wins: {bifrost_wins}")
    print(f"MCRouter wins: {total_comparisons - bifrost_wins}")
    print(f"Bifrost win rate: {(bifrost_wins/total_comparisons)*100:.1f}%")
    print()

    # Best/worst scenarios for each
    if summary_data:
        best_bifrost = max(summary_data, key=lambda x: x['throughput_diff'])
        worst_bifrost = min(summary_data, key=lambda x: x['throughput_diff'])

        print(f"🟢 Best Bifrost scenario: {best_bifrost['scenario']} ({best_bifrost['throughput_diff']:+.1f}%)")
        print(f"🔴 Worst Bifrost scenario: {worst_bifrost['scenario']} ({worst_bifrost['throughput_diff']:+.1f}%)")

    # Save CSV summary
    csv_file = results_path / "benchmark_summary.csv"
    with open(csv_file, 'w') as f:
        f.write("Scenario,Bifrost_OPS,MCRouter_OPS,Throughput_Diff_%,Bifrost_P95_ms,MCRouter_P95_ms,Winner\n")
        for data in summary_data:
            f.write(f"{data['scenario']},{data['bifrost_ops']:.0f},{data['mcrouter_ops']:.0f},"
                   f"{data['throughput_diff']:.1f},{data['bifrost_p95']:.3f},{data['mcrouter_p95']:.3f},"
                   f"{data['winner']}\n")

    print(f"\n💾 Detailed results saved to: {csv_file}")

if __name__ == "__main__":
    analyze_benchmarks("benchmark_results")