#!/bin/bash

# This script profiles context creation overhead to validate lazy context optimization

echo "=== Context Creation Profiling ==="
echo ""

# Build in release mode
echo "Building Rover..."
cargo build --release 2>&1 | grep -E "Finished|Compiling rover"

# Start server in background
echo ""
echo "Starting test server..."
./target/release/rover lazy_context_benchmark_test.lua &
SERVER_PID=$!
sleep 2

# Function to benchmark endpoint
benchmark_endpoint() {
    local path=$1
    local name=$2

    echo ""
    echo "Testing: $name"
    echo "Endpoint: $path"

    # Run short benchmark
    wrk -t4 -c100 -d10s http://localhost:3001$path 2>&1 | grep -E "Requests/sec|Latency"
}

# Test different handler patterns
benchmark_endpoint "/test_none" "No context fields used"
benchmark_endpoint "/test_method" "Only method (simple field)"
benchmark_endpoint "/test_params/123" "Only params (1 closure)"
benchmark_endpoint "/test_all" "All fields (4 closures)"
benchmark_endpoint "/users/123" "Real-world example (1 closure)"

echo ""
echo "=== Results Summary ==="
echo ""
echo "If 'No context' and 'Only method' have similar performance,"
echo "but 'Only params' is slower, then closure creation IS the bottleneck."
echo ""
echo "If all handlers have similar performance, closure creation is NOT significant."

# Cleanup
kill $SERVER_PID 2>/dev/null
wait $SERVER_PID 2>/dev/null

echo ""
echo "Done!"
