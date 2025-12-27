#!/bin/bash

# Comprehensive performance benchmark suite for Rover
# This script builds, runs the echo server, benchmarks it, and provides profiling insights

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
SERVER_PORT=3000
SERVER_PID=""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

cleanup() {
    if [ -n "$SERVER_PID" ]; then
        echo -e "${YELLOW}Stopping server...${NC}"
        kill $SERVER_PID 2>/dev/null || true
        sleep 1
    fi
}

trap cleanup EXIT

echo -e "${GREEN}=== ROVER PERFORMANCE BENCHMARK SUITE ===${NC}"
echo ""

# Step 1: Build in release mode
echo -e "${YELLOW}Step 1: Building Rover in release mode...${NC}"
cd "$PROJECT_ROOT"
cargo build --release
echo -e "${GREEN}✓ Build complete${NC}"
echo ""

# Step 2: Start the server
echo -e "${YELLOW}Step 2: Starting echo server on port $SERVER_PORT...${NC}"
"$PROJECT_ROOT/target/release/rover" "$SCRIPT_DIR/main.lua" &
SERVER_PID=$!
sleep 2

# Verify server is running
if ! kill -0 $SERVER_PID 2>/dev/null; then
    echo -e "${RED}✗ Failed to start server${NC}"
    exit 1
fi
echo -e "${GREEN}✓ Server running (PID: $SERVER_PID)${NC}"
echo ""

# Step 3: Verify server is responding
echo -e "${YELLOW}Step 3: Verifying server is responding...${NC}"
if curl -s http://localhost:$SERVER_PORT/echo > /dev/null; then
    echo -e "${GREEN}✓ Server responding to requests${NC}"
else
    echo -e "${RED}✗ Server not responding${NC}"
    exit 1
fi
echo ""

# Step 4: Run the benchmark
echo -e "${YELLOW}Step 4: Running performance benchmark (2 threads, 100 connections, 30s)...${NC}"
echo ""
cd "$SCRIPT_DIR"
bash test.sh
BENCHMARK_RESULT=$?
echo ""

if [ $BENCHMARK_RESULT -eq 0 ]; then
    echo -e "${GREEN}✓ Benchmark completed successfully${NC}"
else
    echo -e "${RED}✗ Benchmark failed${NC}"
    exit 1
fi

echo ""
echo -e "${GREEN}=== BENCHMARK COMPLETE ===${NC}"
echo ""
echo -e "${YELLOW}Next steps:${NC}"
echo "  1. Review the latency metrics above - p99 latency indicates tail performance"
echo "  2. Check if requests_per_sec is stable across multiple runs"
echo "  3. Look for error spikes which indicate bottlenecks"
echo "  4. To profile code hotspots, use: perf record -p \$PID"
echo "  5. Compare multiple runs to identify performance regressions"
echo ""
