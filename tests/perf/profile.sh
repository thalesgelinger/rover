#!/bin/bash
set -e

echo "🔥 Rover Performance Profiling with Flamegraph"
echo "=============================================="
echo ""

# Check if wrk is available
if ! command -v wrk &> /dev/null; then
    echo "❌ Error: wrk is not installed"
    echo "   Install with: sudo apt-get install wrk"
    exit 1
fi

# Navigate to project root
cd "$(dirname "$0")/../.."

echo "📊 What this test will do:"
echo "  1. Build rover in release mode (optimized)"
echo "  2. Start server with CPU profiling enabled"
echo "  3. Generate load: 4 threads, 100 connections, 30 seconds"
echo "  4. Profile CPU usage to find bottlenecks"
echo "  5. Generate flamegraph.svg visualization"
echo ""
echo "📈 Expected throughput: ~180k req/s"
echo ""

# Clean up any existing flamegraph
rm -f flamegraph.svg perf.data perf.data.old

echo "🔧 Starting server with profiling..."
echo ""

# Start the server with flamegraph in background
# Using sudo because perf requires it
sudo CARGO_PROFILE_RELEASE_DEBUG=true cargo flamegraph --release -- tests/perf/main.lua &
FLAMEGRAPH_PID=$!

# Wait for server to start
echo "⏳ Waiting for server to be ready..."
for i in {1..30}; do
    if curl -s http://localhost:3000/yabadabadoo > /dev/null 2>&1; then
        echo "✅ Server is ready!"
        break
    fi
    if [ $i -eq 30 ]; then
        echo "❌ Server failed to start"
        sudo kill $FLAMEGRAPH_PID 2>/dev/null || true
        exit 1
    fi
    sleep 1
done

echo ""
echo "🚀 Starting load test (30 seconds)..."
echo "   This will hit /yabadabadoo endpoint with:"
echo "   - 4 threads"
echo "   - 100 concurrent connections"
echo "   - ~180k requests/sec expected"
echo ""

# Run the benchmark
cd tests/perf
bash test.sh

echo ""
echo "🛑 Stopping server and generating flamegraph..."

# Stop the server gracefully
sudo kill -INT $FLAMEGRAPH_PID 2>/dev/null || true

# Wait for flamegraph to be generated
sleep 3

cd ../..

if [ -f "flamegraph.svg" ]; then
    echo ""
    echo "✅ Flamegraph generated successfully!"
    echo ""
    echo "📊 Results:"
    echo "   - Flamegraph: flamegraph.svg"
    echo "   - Open with: firefox flamegraph.svg"
    echo "   - Or use any browser to view the interactive SVG"
    echo ""
    echo "🔍 How to read the flamegraph:"
    echo "   - Width = % of CPU time"
    echo "   - Height = call stack depth"
    echo "   - Wide bars = potential bottlenecks"
    echo "   - Click on bars to zoom in"
    echo ""
else
    echo "❌ Flamegraph generation failed"
    echo "   Check if perf is properly installed"
    exit 1
fi
