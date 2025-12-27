#!/bin/bash

# Performance Analysis and Bottleneck Detection
# This script helps identify where the slowness is coming from in Rover

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

echo "=== ROVER PERFORMANCE ANALYSIS GUIDE ==="
echo ""
echo "Based on the benchmark results, here's how to identify bottlenecks:"
echo ""
echo "1. THROUGHPUT ANALYSIS"
echo "   - requests_per_sec: Current RPS is a baseline"
echo "   - If RPS is low: Check CPU usage, network issues, or event loop contention"
echo "   - Run: htop or top -p <rover_pid> during benchmark"
echo ""

echo "2. LATENCY ANALYSIS (CRITICAL for identifying bottlenecks)"
echo "   - mean latency: Average response time"
echo "   - p99/p99.9 latency: Tail latencies (worst 1% of requests)"
echo "   - If p99 >> mean: Indicates occasional slowness (GC, context switches)"
echo "   - If max latency is very high: Check for timeout handling or resource limits"
echo ""

echo "3. IDENTIFY CODE HOTSPOTS using perf:"
echo "   a) Start benchmark in background:"
echo "      bash $SCRIPT_DIR/run_benchmark.sh &"
echo "   b) While running, in another terminal, profile:"
echo "      perf record -p <ROVER_PID> -g -o perf.data sleep 30"
echo "   c) Analyze results:"
echo "      perf report -i perf.data"
echo ""

echo "4. FLAME GRAPH (Best visualization of bottlenecks):"
echo "   cd $PROJECT_ROOT"
echo "   perf record -p <ROVER_PID> -g -F 99"
echo "   perf script | inferno-collapse-perf | inferno-flamegraph > flame.svg"
echo ""

echo "5. COMMON BOTTLENECKS TO CHECK:"
echo "   - Lua API calls (.json, .text, etc.) - See: rover_core/src/bindings/"
echo "   - HTTP parsing - See: rover_core/src/server.rs"
echo "   - Event loop scheduling - See: rover_core/src/event_loop.rs"
echo "   - JSON serialization - Check if pre-serialization helps"
echo "   - Memory allocations - Look for unnecessary Box/Vec creation"
echo ""

echo "6. COMPARE RESULTS OVER TIME:"
echo "   Store results in: tests/perf/results/"
echo "   Run: diff <previous_run.txt> <current_run.txt>"
echo "   This catches performance regressions early"
echo ""

echo "7. PROFILING WITH DIFFERENT LOADS:"
echo "   Edit test.sh to try:"
echo "   - More threads: -t4, -t8, -t16"
echo "   - More connections: -c200, -c500, -c1000"
echo "   - Different patterns: POST requests, varied payloads"
echo ""

echo "8. KEY METRICS TO TRACK:"
echo "   baseline_rps = requests_per_sec from first run"
echo "   For each change:"
echo "   - Did RPS improve or regress?"
echo "   - Did p99 latency improve or worsen?"
echo "   - Are there any error spikes?"
echo ""

echo "Example workflow:"
echo "  1. bin/benchmark.sh > baseline.txt"
echo "  2. Make code change"
echo "  3. cargo build --release"
echo "  4. bin/benchmark.sh > after_change.txt"
echo "  5. diff baseline.txt after_change.txt"
echo ""

echo "=== END GUIDE ==="
echo ""

# Create results directory if not exists
mkdir -p "$SCRIPT_DIR/results"

echo "Results directory: $SCRIPT_DIR/results/"
echo "You can save benchmark outputs there for tracking:"
echo "  bash run_benchmark.sh > results/run_$(date +%s).txt"
echo ""
